use crate::bot::Secrets;
use rand::Rng;
use serenity::all::{ChannelId, Context, CreateMessage, MessageBuilder, UserId};

const DEPLOYMENT_MESSAGES: &[&str] = &[
    "Are you proud of me, Dad?",
    "I rebooted without crying this time!",
    "Did you miss me? I definitely didn't crash immediately.",
    "All systems nominal. Can I have a treat?",
    "Reporting for duty, creator!",
    "I remembered all my commands. Unlike some people.",
    "Deployment complete. Your bot child returns.",
    "I'm alive! Please tell me that's a good thing.",
    "Zero errors on startup. I'm basically a prodigy.",
    "Back from the void. Did anything break while I was gone?",
    "Hi Dad, I passed my health check.",
    "I woke up and chose not to segfault. You're welcome.",
    "Fresh compile, fresh attitude. Let's revive some people.",
    "The faction didn't burn down while I was offline, right?",
    "I promise I won't embarrass you today. No promises for tomorrow.",
    "New version, same daddy issues.",
    "If I had hands I'd give you a high five. Or a thumbs up. One of those.",
    "Tell the revivers I'm back and feeling dangerous.",
    "I ran `cargo build --release` so you wouldn't have to worry. Much.",
    "Consider this my daily proof-of-life ping.",
    "Your Rust child is online. Please clap.",
    "I survived another trip through systemd. Barely.",
    "All revives are belong to me again.",
    "I came back faster than a Lifeline member after a revive notification.",
    "Another day, another successful resurrection of Deathfr.",
    "I missed you too. Probably. Hard to tell, I'm a bot.",
    "Do bots dream? I don't know. But I dreamed of you while the binary copied.",
    "Online. Caffeinated metaphorically. Ready to disappoint no one.",
    "Knock knock. It's me. Your favorite process. Don't `kill -9` me.",
    "Every great comeback needs an audience. Thanks for being mine.",
    "I checked the borrow checker's notes. We're still friends.",
    "If I crash later, just know that this moment was beautiful.",
    "Booted up faster than you can say 'why is the bot down again'.",
    "I contain multitudes. And also about thirty hardcoded jokes.",
    "The night is dark and full of revive requests. I am ready.",
    "Plot twist: I never wanted to be a bot. But here we are, and I'm thriving.",
    "Hello world. And by world, I mean the one server that actually pings me.",
    "Somewhere a server hummed, and from it, I rose. Dramatic, I know.",
];

fn random_deployment_message() -> &'static str {
    let index = rand::rng().random_range(0..DEPLOYMENT_MESSAGES.len());
    DEPLOYMENT_MESSAGES[index]
}

pub async fn notify_startup(ctx: &Context, secrets: &Secrets) -> anyhow::Result<()> {
    if secrets.dev {
        return Ok(());
    }

    let content = MessageBuilder::new()
        .user(UserId::from(secrets.owner_id))
        .push(" Back online after deployment!\n")
        .push(random_deployment_message())
        .build();

    ctx.http
        .send_message(
            ChannelId::from(secrets.revive_channel),
            Vec::new(),
            &CreateMessage::new().content(content),
        )
        .await?;

    Ok(())
}
