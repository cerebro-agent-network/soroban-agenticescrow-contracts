#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token::TokenClient, Address, Env, Symbol,
    symbol_short,
};

const MAX_SIGNERS: usize = 3;
const REQUIRED_SIGNATURES: u32 = 2;

#[contracttype]
pub enum DataKey {
    Owner,
    Token,
    Agent,
    PerTxCap,
    DailyLimit,
    EpochLedgers,
    LastEpochStart,
    EpochSpent,
    Active,
    Signer(u32),
    SignerCount,
}

#[contract]
pub struct AgenticEscrow;

#[contractimpl]
impl AgenticEscrow {
    pub fn initialize(
        env: Env,
        owner: Address,
        token: Address,
        agent: Address,
        per_tx_cap: i128,
        daily_limit: i128,
        epoch_ledgers: u32,
    ) {
        let stored: Option<Address> = env.storage().instance().get(&DataKey::Owner);
        if stored.is_some() {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Agent, &agent);
        env.storage().instance().set(&DataKey::PerTxCap, &per_tx_cap);
        env.storage().instance().set(&DataKey::DailyLimit, &daily_limit);
        env.storage().instance().set(&DataKey::EpochLedgers, &epoch_ledgers);
        env.storage().instance().set(&DataKey::LastEpochStart, &env.ledger().sequence());
        env.storage().instance().set(&DataKey::EpochSpent, &0i128);
        env.storage().instance().set(&DataKey::Active, &true);
        env.storage().instance().set(&DataKey::SignerCount, &0u32);
    }

    pub fn deposit(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        if from != owner {
            panic!("only owner can deposit");
        }
        let token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        TokenClient::new(&env, &token).transfer(&from, &env.current_contract_address(), &amount);
    }

    pub fn configure_agent(
        env: Env,
        agent: Address,
        per_tx_cap: i128,
        daily_limit: i128,
        epoch_ledgers: u32,
    ) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        env.storage().instance().set(&DataKey::Agent, &agent);
        env.storage().instance().set(&DataKey::PerTxCap, &per_tx_cap);
        env.storage().instance().set(&DataKey::DailyLimit, &daily_limit);
        env.storage().instance().set(&DataKey::EpochLedgers, &epoch_ledgers);
        env.storage().instance().set(&DataKey::EpochSpent, &0i128);
        env.storage().instance().set(&DataKey::LastEpochStart, &env.ledger().sequence());
    }

    fn check_epoch_reset(env: &Env) {
        let epoch_ledgers: u32 = env.storage().instance().get(&DataKey::EpochLedgers).unwrap();
        let last_start: u32 = env.storage().instance().get(&DataKey::LastEpochStart).unwrap();
        if env.ledger().sequence() >= last_start + epoch_ledgers {
            env.storage().instance().set(&DataKey::EpochSpent, &0i128);
            env.storage().instance().set(&DataKey::LastEpochStart, &env.ledger().sequence());
        }
    }

    pub fn execute_agent_payment(env: Env, destination: Address, amount: i128, purpose: Symbol) {
        let agent: Address = env.storage().instance().get(&DataKey::Agent).unwrap();
        agent.require_auth();

        let active: bool = env.storage().instance().get(&DataKey::Active).unwrap();
        if !active {
            panic!("agent revoked");
        }

        let per_tx_cap: i128 = env.storage().instance().get(&DataKey::PerTxCap).unwrap();
        if amount > per_tx_cap {
            panic!("exceeds per-tx cap");
        }

        Self::check_epoch_reset(&env);

        let mut epoch_spent: i128 = env.storage().instance().get(&DataKey::EpochSpent).unwrap();
        let daily_limit: i128 = env.storage().instance().get(&DataKey::DailyLimit).unwrap();
        if epoch_spent + amount > daily_limit {
            panic!("exceeds daily limit");
        }

        epoch_spent += amount;
        env.storage().instance().set(&DataKey::EpochSpent, &epoch_spent);

        let token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        TokenClient::new(&env, &token).transfer(
            &env.current_contract_address(),
            &destination,
            &amount,
        );

        env.events().publish(
            (symbol_short!("payment"),),
            (destination, amount, purpose, epoch_spent),
        );
    }

    pub fn revoke_agent(env: Env) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        env.storage().instance().set(&DataKey::Active, &false);
    }

    pub fn add_signer(env: Env, signer: Address) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        let count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();
        if count >= MAX_SIGNERS as u32 {
            panic!("max signers reached");
        }
        for i in 0..count {
            let existing: Address = env.storage().instance().get(&DataKey::Signer(i)).unwrap();
            if existing == signer {
                panic!("signer already exists");
            }
        }
        env.storage().instance().set(&DataKey::Signer(count), &signer);
        env.storage().instance().set(&DataKey::SignerCount, &(count + 1));
    }

    pub fn remove_signer(env: Env, signer: Address) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        let count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();
        let mut found = false;
        for i in 0..count {
            let existing: Address = env.storage().instance().get(&DataKey::Signer(i)).unwrap();
            if existing == signer {
                found = true;
                for j in i..(count - 1) {
                    let next: Address = env.storage().instance().get(&DataKey::Signer(j + 1)).unwrap();
                    env.storage().instance().set(&DataKey::Signer(j), &next);
                }
                env.storage().instance().set(&DataKey::SignerCount, &(count - 1));
                break;
            }
        }
        if !found {
            panic!("signer not found");
        }
    }

    pub fn emergency_panic(env: Env, signer_a: Address, signer_b: Address) {
        signer_a.require_auth();
        signer_b.require_auth();
        if signer_a == signer_b {
            panic!("need two distinct signers");
        }
        let count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();
        let mut found_a = false;
        let mut found_b = false;
        for i in 0..count {
            let existing: Address = env.storage().instance().get(&DataKey::Signer(i)).unwrap();
            if existing == signer_a {
                found_a = true;
            }
            if existing == signer_b {
                found_b = true;
            }
        }
        if !found_a || !found_b {
            panic!("signers not authorized");
        }
        env.storage().instance().set(&DataKey::Active, &false);
    }

    pub fn get_remaining_allowance(env: Env) -> i128 {
        let active: bool = env.storage().instance().get(&DataKey::Active).unwrap_or(false);
        if !active {
            return 0;
        }
        Self::check_epoch_reset(&env);
        let daily_limit: i128 = env.storage().instance().get(&DataKey::DailyLimit).unwrap();
        let epoch_spent: i128 = env.storage().instance().get(&DataKey::EpochSpent).unwrap();
        (daily_limit - epoch_spent).max(0)
    }

    pub fn get_balance(env: Env) -> i128 {
        let token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        TokenClient::new(&env, &token).balance(&env.current_contract_address())
    }

    pub fn token(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Token).unwrap()
    }

    pub fn owner(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Owner).unwrap()
    }

    pub fn agent(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Agent).unwrap()
    }

    pub fn is_active(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Active).unwrap()
    }

    pub fn config(env: Env) -> (i128, i128, u32) {
        let cap: i128 = env.storage().instance().get(&DataKey::PerTxCap).unwrap();
        let limit: i128 = env.storage().instance().get(&DataKey::DailyLimit).unwrap();
        let epoch: u32 = env.storage().instance().get(&DataKey::EpochLedgers).unwrap();
        (cap, limit, epoch)
    }
}

#[cfg(test)]
mod test;
