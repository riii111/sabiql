use crate::update::action::Action;

use super::actions;
use super::types::{
    BrowseVimContext, JsonDetailVimContext, SqlModalVimContext, VimCommand, VimSurfaceContext,
};

pub fn surface(command: VimCommand, ctx: VimSurfaceContext) -> Option<Action> {
    match ctx {
        VimSurfaceContext::Browse(ctx) => browse(command, ctx),
        VimSurfaceContext::SqlModal(ctx) => sql(command, ctx),
        VimSurfaceContext::JsonDetail(ctx) => json(command, ctx),
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

fn sql(command: VimCommand, ctx: SqlModalVimContext) -> Option<Action> {
    actions::sql::command(command, ctx)
}

fn json(command: VimCommand, ctx: JsonDetailVimContext) -> Option<Action> {
    actions::json::command(command, ctx)
}
