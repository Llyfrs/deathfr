use serenity::all::CreateCommand;

pub fn revive_me() {
    println!("Revive me!");
}


pub fn register_commands() -> CreateCommand {
    CreateCommand::new("reviveme")
        .description("Ask for a revive")
}


