# Fuzio Option Trading

This is a contract that will run the Fuzio Prediction games. They take advantage of the Sei oracle to get price information of an asset and allows people to bet if the price of a certain asset will go up or down in the next X minutes. After the game is finished, the users that got the right prediction will receive part of the total pot (according to their share of the total bet).

Each contract will be 1 prediction game, and we will provide the rounds duration, the gaming fee (% sent to dev wallets), the token that we are betting against, and the token that we will use as bet currency and prize reward.

We will run a service in a server that will periodically close rounds using the admin wallet to keep the game on going indefinitely.

# Instantiation

The contract can be instantiated with the following messages

```
{
    "next_round_seconds": "<ROUNDS_DURATION>",
    "minimum_bet": "<MIN_BET_AMOUNT>",
    "gaming_fee": "<GAMING_FEE>",     // 1 = 0.01%
    "token_denom": "<DENOM>",
    "bet_token_denom": "<DENOM>",
    "dev_wallet_list": [{"address": "<DEV_WALLET_1>", "ratio": "<RATIO_1>"},...]
}
```

# Messages

### BetBull

User bets that the price will go up.

### BetBear

User bets that the price will go down.

### CollectWinnings

Collect earnings of a specific round that was won.

### CollectWinningsRound

Collect all earnings of successful bets.

### Halt (Admin only)

Pause the game.

### Resume (Admin only)

Resume the game.

### CloseRound (Admin only)

Close current round if round duration is over.

### UpdateConfig (Admin only)

Allows the admin to modify the config of the game.

### Add Admin (Admin only)

Add a new admin to the admin list.

### RemoveAdmin (Admin only)

Remove admin from the admin list.

### ModifyDevWallet (Admin only)

Provides new wallets for dev rewards.