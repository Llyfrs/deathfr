# Deathf

This is a discord bot developed for the [Lifeline](https://www.torn.com/factions.php?step=profile&ID=38481) faction to help with revive requests and contract management. This was both a reqested bot and a project for my universitou course **Programming in Rust**, I was encourate to avoid using prexisting libraries for registering commands to expand the scope of the project, and while good for learning rust, it does make the code way harder to mantain long term. 

Yet mantaning it I do, the bost is currently installed on 10 different discord servers and it requalry used by both leadership to manage contracts, that by contracted factions to request revives. 

/contract start
Creates a new contract and immediately starts it. Takes contract_name and faction_id and min_chance as arguments.
contract_name is used as a identifier in list so I recommend naming it something meaningful like served faction name + date.
faction_id is the faction you want to track revives for (if both defense and offensive revives are provided two different contracts need to be made
min_chance is the minimum revive chance of success to count for payment
faction_cut is the cut the faction gets from the contract (defaults to 10% if not set)
Returns contract ID that can be used for ending the contract, and is to be passed to the contracted faction so they can generate report if they want to.
/contract end
Ends a contract. Takes contract_id as argument. Contract ID is returned when creating a new contract.
/contract list
Lists all contracts. Takes status as argument. Status can be active, ended, or all. Contracts are separated in to pages by 10
/report
Generate contract report
/help
Get a list of all available commands



# TODO 

- Allow requesting reviews for other players 
- Move verification in to its own function for consistency
- Log Revives in to a chanel
- Implement /log function to see all logs for contract
