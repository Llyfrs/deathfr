# Deathfr

This is a Discord bot developed for the [Lifeline](https://www.torn.com/factions.php?step=profile&ID=38481) faction to help with revive requests and contract management. This was both a requested bot and a project for my university course **Programming in Rust**. I was encouraged to avoid using preexisting libraries for registering commands to expand the scope of the project, and while good for learning Rust, it does make the code way harder to maintain long term.

Yet maintain it I do: the bot is currently installed on 10 different Discord servers, and it is regularly used by both leadership to manage contracts and by contracted factions to request revives.

## Commands

`/contract start`  
Creates a new contract and immediately starts it. Takes the following arguments:
- `contract_name` is used as an identifier in the list, so I recommend naming it something meaningful like served faction name + date.
- `faction_id` is the faction you want to track revives for (if both defense and offensive revives are provided two different contracts need to be made).
- `min_chance` is the minimum revive chance of success to count for payment.
- `faction_cut` is the cut the faction gets from the contract (defaults to 10% if not set).
Returns contract ID that can be used for ending the contract, and is to be passed to the contracted faction so they can generate a report if they want to.

`/contract end`  
Ends a contract. Takes `contract_id` as an argument. Contract ID is returned when creating a new contract.

`/contract list`  
Lists all contracts. Takes `status` as an argument. Status can be active, ended, or all. Contracts are separated into pages by 10.

`/report`  
Generate contract report.

`/submitkey`  
Opens a form to submit your Torn API key (donation). Deathfr uses these keys only for authentication when using `/reviveme` and basic validity checks; donated keys are rotated and rate limited to 10 requests per minute.

`/help`  
Get a list of all available commands.



## TODO

- Allow requesting reviews for other players 
- Move verification into its own function for consistency
- Log Revives into a channel
- Implement /log function to see all logs for contract
