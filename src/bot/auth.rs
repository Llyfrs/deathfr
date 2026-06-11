use crate::bot::data::{Context, Secrets};

/// Access level of a user invoking a command, ordered from least to most privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessLevel {
    /// Anyone, anywhere
    Public,
    /// Interaction happening inside the revive faction guild
    FactionGuild,
    /// User is in the configured admin list
    Admin,
    /// User is the bot owner
    Owner,
}

/// Returns the highest access level the user has for the given invocation context.
pub fn access_level(secrets: &Secrets, user_id: u64, guild_id: Option<u64>) -> AccessLevel {
    if user_id == secrets.owner_id {
        return AccessLevel::Owner;
    }
    if secrets.admins.contains(&user_id) {
        return AccessLevel::Admin;
    }
    if let Some(g) = guild_id {
        if secrets.is_revive_faction_guild(g) {
            return AccessLevel::FactionGuild;
        }
    }
    AccessLevel::Public
}

/// Convenience wrapper that derives the access level straight from a poise context.
pub fn level_of(ctx: &Context<'_>) -> AccessLevel {
    access_level(
        &ctx.data().secrets,
        ctx.author().id.get(),
        ctx.guild_id().map(|g| g.get()),
    )
}
