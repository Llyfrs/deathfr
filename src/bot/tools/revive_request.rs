use serenity::all::MessageBuilder;

#[derive(Debug, Clone)]
pub struct ReviveRequest {
    pub requester_discord_id: Option<u64>,
    pub requester_name: String,
    pub requester_torn_player_id: u64,
    pub faction_id: u64,
    pub faction_name: String,
    pub min_contract_chance: Option<u64>,
}

pub fn build_revive_request_message(request: &ReviveRequest) -> String {
    let mut message = MessageBuilder::new();

    message.push("Revive request by").push(format!(
        " [{} [{}]]({}) ",
        request.requester_name,
        request.requester_torn_player_id,
        player_link(request.requester_torn_player_id)
    ));

    if request.faction_id != 0 {
        message.push("from").push(format!(
            " [{}]({}) ",
            request.faction_name,
            faction_link(request.faction_id)
        ));
    }

    if let Some(min_chance) = request.min_contract_chance {
        message.push_bold("\nThis player is under contract ");
        message.push(format!("Revive above {}% chance", min_chance));
    }

    message.build()
}

fn player_link(id: u64) -> String {
    format!("https://www.torn.com/profiles.php?XID={}", id)
}

fn faction_link(id: u64) -> String {
    format!("https://www.torn.com/factions.php?step=profile&ID={}", id)
}

#[cfg(test)]
mod tests {
    use super::{build_revive_request_message, ReviveRequest};

    #[test]
    fn revive_message_contains_player_and_faction_details() {
        let request = ReviveRequest {
            requester_discord_id: Some(123),
            requester_name: "Tester".to_string(),
            requester_torn_player_id: 456,
            faction_id: 789,
            faction_name: "Lifeline".to_string(),
            min_contract_chance: None,
        };

        let message = build_revive_request_message(&request);

        assert!(message.contains("Revive request by [Tester [456]]"));
        assert!(message.contains("https://www.torn.com/profiles.php?XID=456"));
        assert!(message.contains("from [Lifeline]"));
        assert!(message.contains("https://www.torn.com/factions.php?step=profile&ID=789"));
    }

    #[test]
    fn revive_message_contains_contract_requirement_when_present() {
        let request = ReviveRequest {
            requester_discord_id: Some(123),
            requester_name: "Tester".to_string(),
            requester_torn_player_id: 456,
            faction_id: 0,
            faction_name: String::new(),
            min_contract_chance: Some(85),
        };

        let message = build_revive_request_message(&request);

        assert!(message.contains("This player is under contract"));
        assert!(message.contains("Revive above 85% chance"));
    }
}
