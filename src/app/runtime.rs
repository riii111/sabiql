use std::cell::RefCell;
use std::time::Instant;

use color_eyre::eyre::Result;

use crate::cmd::completion_engine::CompletionEngine;
use crate::cmd::effect::Effect;
use crate::cmd::runner::EffectRunner;
use crate::model::app_state::AppState;
use crate::ports::Renderer;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::reducer::reduce;

pub struct AppRuntime<'a> {
    effect_runner: &'a EffectRunner,
    completion_engine: &'a RefCell<CompletionEngine>,
    services: &'a AppServices,
}

impl<'a> AppRuntime<'a> {
    pub fn new(
        effect_runner: &'a EffectRunner,
        completion_engine: &'a RefCell<CompletionEngine>,
        services: &'a AppServices,
    ) -> Self {
        Self {
            effect_runner,
            completion_engine,
            services,
        }
    }

    pub async fn dispatch<T: Renderer>(
        &self,
        action: Action,
        state: &mut AppState,
        renderer: &mut T,
    ) -> Result<()> {
        let now = Instant::now();
        let is_animation_tick = matches!(action, Action::Render);
        if is_animation_tick {
            state.clear_expired_timers(now);
        }

        let mut effects = reduce(state, action, now, self.services);
        if state.render_dirty {
            if !is_animation_tick {
                state.clear_expired_timers(now);
            }
            effects.push(Effect::Render);
        }

        self.flush(effects, state, renderer).await
    }

    pub fn services(&self) -> &AppServices {
        self.services
    }

    #[allow(
        clippy::print_stderr,
        reason = "last-resort fallback when effect dispatch exceeds recursion limit"
    )]
    pub async fn flush<T: Renderer>(
        &self,
        effects: Vec<Effect>,
        state: &mut AppState,
        renderer: &mut T,
    ) -> Result<()> {
        let mut pending = self
            .effect_runner
            .run(
                effects,
                renderer,
                state,
                self.completion_engine,
                self.services,
            )
            .await?;
        state.clear_dirty();

        const MAX_DEPTH: usize = 16;
        let mut depth = 0;
        while !pending.is_empty() && depth < MAX_DEPTH {
            depth += 1;
            let mut next = Vec::new();
            for action in pending {
                let now = Instant::now();
                let mut effects = reduce(state, action, now, self.services);
                if state.render_dirty {
                    state.clear_expired_timers(now);
                    effects.push(Effect::Render);
                }
                next.extend(
                    self.effect_runner
                        .run(
                            effects,
                            renderer,
                            state,
                            self.completion_engine,
                            self.services,
                        )
                        .await?,
                );
                state.clear_dirty();
            }
            pending = next;
        }

        if depth >= MAX_DEPTH && !pending.is_empty() {
            eprintln!(
                "DispatchActions recursion depth exceeded ({MAX_DEPTH}), \
                 falling back to channel for {} remaining actions",
                pending.len()
            );
            for action in pending {
                if let Err(error) = self.effect_runner.action_tx().try_send(action) {
                    eprintln!("DispatchActions fallback: channel full, dropping action: {error}");
                }
            }
        }

        Ok(())
    }
}
