#![cfg(test)]

extern crate std;

use crate::{AgenticEscrow, AgenticEscrowClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _, LedgerInfo},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let agent = Address::generate(&env);

    let sac_contract = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac_contract.address();
    let sac = StellarAssetClient::new(&env, &token);

    let contract_id = env.register(AgenticEscrow, ());
    let client = AgenticEscrowClient::new(&env, &contract_id);

    client.initialize(
        &owner,
        &token,
        &agent,
        &1_000_000_i128,
        &5_000_000_i128,
        &100_u32,
    );

    sac.mint(&owner, &10_000_000_000);
    client.deposit(&owner, &5_000_000_000);

    (env, contract_id, owner, agent, token)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();

    let contract_id = env.register(AgenticEscrow, ());
    let client = AgenticEscrowClient::new(&env, &contract_id);

    client.initialize(&owner, &token, &agent, &1_000_000, &5_000_000, &100);

    assert_eq!(client.owner(), owner);
    assert_eq!(client.agent(), agent);
    assert!(client.is_active());
    assert_eq!(client.get_remaining_allowance(), 5_000_000);
}

#[test]
fn test_deposit() {
    let (env, contract_id, _owner, _agent, token) = setup();

    let balance = TokenClient::new(&env, &token).balance(&contract_id);
    assert_eq!(balance, 5_000_000_000);
}

#[test]
fn test_valid_agent_payment() {
    let (env, _contract_id, _owner, agent, token) = setup();

    let client = AgenticEscrowClient::new(&env, &agent);
    let destination = Address::generate(&env);

    client.execute_agent_payment(&destination, &500_000, &symbol_short!("compute"));

    let dest_balance = TokenClient::new(&env, &token).balance(&destination);
    assert_eq!(dest_balance, 500_000);

    let remaining = client.get_remaining_allowance();
    assert_eq!(remaining, 4_500_000);
}

#[test]
fn test_agent_payment_exceeds_per_tx_cap() {
    let (env, _contract_id, _owner, agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &agent);
    let destination = Address::generate(&env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_agent_payment(&destination, &2_000_000, &symbol_short!("compute"));
    }));
    assert!(result.is_err());
}

#[test]
fn test_agent_payment_exceeds_daily_limit() {
    let (env, _contract_id, _owner, agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &agent);
    let destination = Address::generate(&env);

    client.execute_agent_payment(&destination, &3_000_000, &symbol_short!("api1"));

    let destination2 = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_agent_payment(&destination2, &3_000_000, &symbol_short!("api2"));
    }));
    assert!(result.is_err());
}

#[test]
fn test_epoch_reset_allows_new_spends() {
    let (env, _contract_id, _owner, agent, token) = setup();
    let client = AgenticEscrowClient::new(&env, &agent);
    let destination = Address::generate(&env);

    client.execute_agent_payment(&destination, &3_000_000, &symbol_short!("compute"));

    env.ledger().set(LedgerInfo {
        sequence_number: 200,
        ..Default::default()
    });

    let destination2 = Address::generate(&env);
    client.execute_agent_payment(&destination2, &3_000_000, &symbol_short!("compute"));

    let dest2_balance = TokenClient::new(&env, &token).balance(&destination2);
    assert_eq!(dest2_balance, 3_000_000);
}

#[test]
fn test_owner_revoke_agent() {
    let (env, _contract_id, owner, agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &owner);

    client.revoke_agent();
    assert!(!client.is_active());

    let destination = Address::generate(&env);
    let client_agent = AgenticEscrowClient::new(&env, &agent);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client_agent.execute_agent_payment(&destination, &500_000, &symbol_short!("compute"));
    }));
    assert!(result.is_err());
}

#[test]
fn test_multi_sig_emergency_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    let contract_id = env.register(AgenticEscrow, ());
    let client = AgenticEscrowClient::new(&env, &owner);

    client.initialize(&owner, &token, &agent, &1_000_000, &5_000_000, &100);

    client.add_signer(&signer_a);
    client.add_signer(&signer_b);
    client.add_signer(&signer_c);

    client.emergency_panic(&signer_a, &signer_b);
    assert!(!client.is_active());
}

#[test]
fn test_multi_sig_single_signer_not_enough() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let signer_a = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    let contract_id = env.register(AgenticEscrow, ());
    let client = AgenticEscrowClient::new(&env, &owner);

    client.initialize(&owner, &token, &agent, &1_000_000, &5_000_000, &100);
    client.add_signer(&signer_a);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.emergency_panic(&signer_a, &signer_a);
    }));
    assert!(result.is_err());
}

#[test]
fn test_unauthorized_signer_cannot_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let intruder = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    let contract_id = env.register(AgenticEscrow, ());
    let client = AgenticEscrowClient::new(&env, &owner);

    client.initialize(&owner, &token, &agent, &1_000_000, &5_000_000, &100);
    client.add_signer(&signer_a);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.emergency_panic(&signer_a, &intruder);
    }));
    assert!(result.is_err());
}

#[test]
fn test_configure_agent_updates_settings() {
    let (env, _contract_id, owner, _agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &owner);

    let new_agent = Address::generate(&env);
    client.configure_agent(&new_agent, &2_000_000, &10_000_000, &200);

    assert_eq!(client.agent(), new_agent);
    let (cap, limit, epoch) = client.config();
    assert_eq!(cap, 2_000_000);
    assert_eq!(limit, 10_000_000);
    assert_eq!(epoch, 200);
}

#[test]
fn test_remaining_allowance_after_revoke() {
    let (env, _contract_id, owner, _agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &owner);

    client.revoke_agent();
    assert_eq!(client.get_remaining_allowance(), 0);
}

#[test]
fn test_full_daily_spend() {
    let (env, _contract_id, _owner, agent, _token) = setup();
    let client = AgenticEscrowClient::new(&env, &agent);
    let destination = Address::generate(&env);

    client.execute_agent_payment(&destination, &5_000_000, &symbol_short!("compute"));

    assert_eq!(client.get_remaining_allowance(), 0);
}
