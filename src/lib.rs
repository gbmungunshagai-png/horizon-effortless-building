//! Effortless Building for Horizon: a port of Requios's Effortless Building (LGPL-2.1,
//! https://github.com/Requios/effortless-building, branch 1.21). Build modes and their options,
//! the radial menu, Mirror / Array / Radial Mirror modifiers, quick replace, undo and redo.
//! Client-only: blocks are set with `/setblock` where the player may run it, else with vanilla clicks.
mod apply;
mod client;
mod modes;
mod modifiers;
mod ui;

use mod_api::{
    self, ClientChatReceivedEvent, ClientHost, ClientLevelLoadEvent, ClientTickPreEvent, Flow, GuiRenderPostEvent,
    Hooks, InteractionKeyMappingTriggeredEvent, RegistrationContext, Registrar, RenderLevelStage, RenderLevelStageEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent,
};

#[derive(Default)]
struct Mod;

/// Whether the screen these hooks see is ours, and which of ours.
fn our_screen(host: &dyn ClientHost) -> Option<ui::Screen> {
    if host.mod_screen() != Some(client::MOD_ID) {
        return None;
    }
    client::STATE.lock().unwrap().screen
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        client::register_keys(r);
    }

    fn client_level_load(&self, _host: &mut dyn ClientHost, _event: &mut ClientLevelLoadEvent<'_>) {
        apply::reset();
    }

    fn client_chat_received(&self, _host: &mut dyn ClientHost, event: &mut ClientChatReceivedEvent<'_>) -> Flow {
        if event.system && apply::on_chat(event.message) {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        client::tick(host);
    }

    fn interaction_key_mapping_triggered(
        &self,
        host: &mut dyn ClientHost,
        event: &mut InteractionKeyMappingTriggeredEvent,
    ) -> Flow {
        client::interact(host, event)
    }

    fn render_level_stage(&self, host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        if event.stage == RenderLevelStage::AfterLevel {
            client::render(host, &mut *event.draw);
        }
    }

    fn gui_render_post(&self, host: &dyn ClientHost, event: &mut GuiRenderPostEvent<'_>) {
        if !event.gui_hidden {
            client::hud(host, &mut *event.batch);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        match our_screen(host) {
            Some(ui::Screen::Radial) => {
                ui::draw_radial(host, &mut *event.draw, event.screen.mouse, event.screen.partial_tick)
            }
            Some(ui::Screen::Modifiers) => ui::draw_modifiers(host, &mut *event.draw, event.screen.mouse),
            None => {}
        }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        match our_screen(host) {
            Some(ui::Screen::Radial) => {
                ui::radial_click(host);
                Flow::Cancel
            }
            Some(ui::Screen::Modifiers) if event.button == 0 => {
                ui::modifiers_click(host, event.screen.mouse);
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if our_screen(host) == Some(ui::Screen::Modifiers) {
            ui::modifiers_scroll(event.screen.mouse, event.scroll[1], event.modifiers);
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        // The Modifier Menu key closes the screen it opened.
        if our_screen(host) == Some(ui::Screen::Modifiers) {
            if let Some((key, _)) = host.mod_key(client::MOD_ID, "modifiers") {
                if key == event.key {
                    client::STATE.lock().unwrap().deferred.push(client::Deferred::CloseScreen);
                    return Flow::Cancel;
                }
            }
        }
        Flow::Continue
    }
}
