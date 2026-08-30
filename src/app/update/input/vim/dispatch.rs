use crate::update::action::Action;

use super::actions;
use super::types::{BrowseVimContext, VimCommand, VimSurfaceContext};

pub fn surface(command: VimCommand, ctx: VimSurfaceContext) -> Option<Action> {
    match ctx {
        VimSurfaceContext::Browse(ctx) => browse(command, ctx),
        VimSurfaceContext::SqlModal(ctx) => actions::sql::command(command, ctx),
        VimSurfaceContext::JsonDetail(ctx) => actions::json::command(command, ctx),
    }
}

fn browse(command: VimCommand, ctx: BrowseVimContext) -> Option<Action> {
    match command {
        VimCommand::Navigation(navigation) => Some(actions::browse::navigation(navigation, ctx)),
        VimCommand::ModeTransition(transition) => {
            Some(actions::browse::mode_transition(transition, ctx))
        }
        VimCommand::SearchContinuation(_) => None,
        VimCommand::Operator(operator) => actions::browse::operator(operator, ctx),
    }
}
