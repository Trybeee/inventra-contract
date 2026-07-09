#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

// ============================================================
// TEST HELPERS
// ============================================================

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InventraContract, ());
    let admin       = Address::generate(&env);
    let owner       = Address::generate(&env);
    let owner2      = Address::generate(&env);

    let client = InventraContractClient::new(&env, &contract_id);
    client.init(&admin);

    (env, contract_id, admin, owner, owner2)
}

fn register_warehouse_and_item(
    env: &Env,
    client: &InventraContractClient,
    admin: &Address,
    owner: &Address,
    warehouse_id: &str,
    item_id: &str,
    quantity: u64,
) {
    client.register_warehouse(
        admin,
        &String::from_str(env, warehouse_id),
        &String::from_str(env, "Main Warehouse"),
        admin,
    );
    client.register_item(
        owner,
        &String::from_str(env, item_id),
        &String::from_str(env, "Steel Rod"),
        &ItemCategory::RawMaterial,
        &String::from_str(env, warehouse_id),
        &quantity,
        &String::from_str(env, "kg"),
        &String::from_str(env, "BATCH-001"),
    );
}

// ============================================================
// INIT TESTS
// ============================================================

#[test]
fn test_init_success() {
    let (env, contract_id, admin, _, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);
    let (items, warehouses, transfers) = client.get_stats();
    assert_eq!(items, 0);
    assert_eq!(warehouses, 0);
    assert_eq!(transfers, 0);
}

// ============================================================
// WAREHOUSE TESTS
// ============================================================

#[test]
fn test_register_warehouse_success() {
    let (env, contract_id, admin, _, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    client.register_warehouse(
        &admin,
        &String::from_str(&env, "WH-001"),
        &String::from_str(&env, "Lagos Distribution Centre"),
        &admin,
    );

    let warehouse = client.get_warehouse(&String::from_str(&env, "WH-001"));
    assert!(warehouse.is_active);
    assert_eq!(warehouse.item_count, 0);

    let (_, warehouses, _) = client.get_stats();
    assert_eq!(warehouses, 1);
}

#[test]
#[should_panic(expected = "warehouse already registered")]
fn test_register_duplicate_warehouse() {
    let (env, contract_id, admin, _, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    let wh_id = String::from_str(&env, "WH-DUP");
    client.register_warehouse(&admin, &wh_id, &String::from_str(&env, "Warehouse"), &admin);
    client.register_warehouse(&admin, &wh_id, &String::from_str(&env, "Warehouse"), &admin);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn test_register_warehouse_unauthorized() {
    let (env, contract_id, _, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    client.register_warehouse(
        &owner,
        &String::from_str(&env, "WH-001"),
        &String::from_str(&env, "Warehouse"),
        &owner,
    );
}

#[test]
fn test_deactivate_and_reactivate_warehouse() {
    let (env, contract_id, admin, _, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);
    let wh_id = String::from_str(&env, "WH-001");

    client.register_warehouse(&admin, &wh_id, &String::from_str(&env, "WH"), &admin);
    client.deactivate_warehouse(&admin, &wh_id);

    let warehouse = client.get_warehouse(&wh_id);
    assert!(!warehouse.is_active);

    client.reactivate_warehouse(&admin, &wh_id);
    let warehouse = client.get_warehouse(&wh_id);
    assert!(warehouse.is_active);
}

// ============================================================
// ITEM REGISTRATION TESTS
// ============================================================

#[test]
fn test_register_item_success() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(
        &env, &client, &admin, &owner,
        "WH-001", "ITEM-001", 500,
    );

    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.quantity, 500);
    assert_eq!(item.status, ItemStatus::Active);
    assert_eq!(item.owner, owner);

    // Warehouse item count incremented
    let warehouse = client.get_warehouse(&String::from_str(&env, "WH-001"));
    assert_eq!(warehouse.item_count, 1);

    let (items, _, _) = client.get_stats();
    assert_eq!(items, 1);
}

#[test]
#[should_panic(expected = "item already registered")]
fn test_register_duplicate_item() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-DUP", 100);
    // Try again with same ID
    client.register_item(
        &owner,
        &String::from_str(&env, "ITEM-DUP"),
        &String::from_str(&env, "Another Item"),
        &ItemCategory::FinishedGoods,
        &String::from_str(&env, "WH-001"),
        &50u64,
        &String::from_str(&env, "units"),
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "warehouse is not active")]
fn test_register_item_inactive_warehouse() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    let wh_id = String::from_str(&env, "WH-INACTIVE");
    client.register_warehouse(&admin, &wh_id, &String::from_str(&env, "WH"), &admin);
    client.deactivate_warehouse(&admin, &wh_id);

    client.register_item(
        &owner,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "Steel"),
        &ItemCategory::RawMaterial,
        &wh_id,
        &100u64,
        &String::from_str(&env, "kg"),
        &String::from_str(&env, ""),
    );
}

#[test]
fn test_verify_ownership() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    let item_id = String::from_str(&env, "ITEM-001");
    assert!(client.verify_ownership(&item_id, &owner));
    assert!(!client.verify_ownership(&item_id, &owner2));
}

#[test]
fn test_item_exists() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    assert!(client.item_exists(&String::from_str(&env, "ITEM-001")));
    assert!(!client.item_exists(&String::from_str(&env, "ITEM-999")));
}

// ============================================================
// TRANSFER TESTS
// ============================================================

#[test]
fn test_full_transfer_success() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    // Register two warehouses
    client.register_warehouse(&admin, &String::from_str(&env, "WH-001"),
        &String::from_str(&env, "Source WH"), &admin);
    client.register_warehouse(&admin, &String::from_str(&env, "WH-002"),
        &String::from_str(&env, "Dest WH"), &admin);

    client.register_item(
        &owner,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "Copper Wire"),
        &ItemCategory::RawMaterial,
        &String::from_str(&env, "WH-001"),
        &200u64,
        &String::from_str(&env, "kg"),
        &String::from_str(&env, "BATCH-A"),
    );

    // Full transfer of 200kg to owner2 at WH-002
    client.transfer_item(
        &owner,
        &owner2,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "WH-002"),
        &200u64,
        &String::from_str(&env, "TXF-001"),
        &String::from_str(&env, "Inter-warehouse restock"),
    );

    // Ownership transferred
    assert!(client.verify_ownership(&String::from_str(&env, "ITEM-001"), &owner2));
    assert!(!client.verify_ownership(&String::from_str(&env, "ITEM-001"), &owner));

    // New warehouse
    let new_wh = client.get_item_warehouse(&String::from_str(&env, "ITEM-001"));
    assert_eq!(new_wh, String::from_str(&env, "WH-002"));

    // Transfer record written
    let record = client.get_transfer(&String::from_str(&env, "TXF-001"));
    assert_eq!(record.quantity, 200);
    assert_eq!(record.from_warehouse, String::from_str(&env, "WH-001"));
    assert_eq!(record.to_warehouse, String::from_str(&env, "WH-002"));

    let (_, _, transfers) = client.get_stats();
    assert_eq!(transfers, 1);
}

#[test]
fn test_partial_transfer_keeps_remaining_stock() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    client.register_warehouse(&admin, &String::from_str(&env, "WH-001"),
        &String::from_str(&env, "WH"), &admin);
    client.register_warehouse(&admin, &String::from_str(&env, "WH-002"),
        &String::from_str(&env, "WH2"), &admin);

    client.register_item(
        &owner,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "Cement Bags"),
        &ItemCategory::RawMaterial,
        &String::from_str(&env, "WH-001"),
        &1000u64,
        &String::from_str(&env, "bags"),
        &String::from_str(&env, ""),
    );

    // Partial transfer of 300 bags
    client.transfer_item(
        &owner, &owner2,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "WH-002"),
        &300u64,
        &String::from_str(&env, "TXF-001"),
        &String::from_str(&env, "Partial delivery"),
    );

    // Source still has 700 bags and original ownership
    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.quantity, 700);
    assert_eq!(item.owner, owner);
}

#[test]
#[should_panic(expected = "transfer quantity exceeds available stock")]
fn test_transfer_exceeds_stock() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);
    client.register_warehouse(&admin, &String::from_str(&env, "WH-002"),
        &String::from_str(&env, "WH2"), &admin);

    client.transfer_item(
        &owner, &owner2,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "WH-002"),
        &999u64,
        &String::from_str(&env, "TXF-001"),
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "unauthorized: caller is not the item owner")]
fn test_transfer_unauthorized() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);
    client.register_warehouse(&admin, &String::from_str(&env, "WH-002"),
        &String::from_str(&env, "WH2"), &admin);

    // owner2 tries to transfer owner's item
    client.transfer_item(
        &owner2, &owner2,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "WH-002"),
        &50u64,
        &String::from_str(&env, "TXF-001"),
        &String::from_str(&env, ""),
    );
}

// ============================================================
// STOCK ADJUSTMENT TESTS
// ============================================================

#[test]
fn test_adjust_stock_success() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 500);

    client.adjust_stock(
        &owner,
        &String::from_str(&env, "ITEM-001"),
        &480u64,
        &String::from_str(&env, "ADJ-001"),
        &String::from_str(&env, "Physical count — 20kg wastage"),
    );

    assert_eq!(client.get_stock_quantity(&String::from_str(&env, "ITEM-001")), 480);

    let adj = client.get_adjustment(&String::from_str(&env, "ADJ-001"));
    assert_eq!(adj.previous_quantity, 500);
    assert_eq!(adj.new_quantity, 480);
}

#[test]
fn test_adjust_stock_to_zero_marks_consumed() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    client.adjust_stock(
        &owner,
        &String::from_str(&env, "ITEM-001"),
        &0u64,
        &String::from_str(&env, "ADJ-001"),
        &String::from_str(&env, "All stock consumed"),
    );

    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.status, ItemStatus::Consumed);
}

#[test]
fn test_admin_can_adjust_any_item() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    // Admin adjusts owner's item
    client.adjust_stock(
        &admin,
        &String::from_str(&env, "ITEM-001"),
        &90u64,
        &String::from_str(&env, "ADJ-001"),
        &String::from_str(&env, "Audit correction"),
    );

    assert_eq!(client.get_stock_quantity(&String::from_str(&env, "ITEM-001")), 90);
}

// ============================================================
// FLAG / CONSUME TESTS
// ============================================================

#[test]
fn test_flag_and_unflag_item() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    client.flag_item(
        &admin,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "Discrepancy in stock count"),
    );

    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.status, ItemStatus::Flagged);

    client.unflag_item(&admin, &String::from_str(&env, "ITEM-001"));

    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.status, ItemStatus::Active);
}

#[test]
fn test_consume_item() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);

    client.consume_item(&owner, &String::from_str(&env, "ITEM-001"));

    let item = client.get_item(&String::from_str(&env, "ITEM-001"));
    assert_eq!(item.status, ItemStatus::Consumed);
}

#[test]
#[should_panic(expected = "only active items can be consumed")]
fn test_cannot_consume_already_consumed() {
    let (env, contract_id, admin, owner, _) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    register_warehouse_and_item(&env, &client, &admin, &owner, "WH-001", "ITEM-001", 100);
    client.consume_item(&owner, &String::from_str(&env, "ITEM-001"));
    client.consume_item(&owner, &String::from_str(&env, "ITEM-001")); // second consume
}

// ============================================================
// STATS TESTS
// ============================================================

#[test]
fn test_stats_accumulate_correctly() {
    let (env, contract_id, admin, owner, owner2) = setup();
    let client = InventraContractClient::new(&env, &contract_id);

    client.register_warehouse(&admin, &String::from_str(&env, "WH-001"),
        &String::from_str(&env, "WH1"), &admin);
    client.register_warehouse(&admin, &String::from_str(&env, "WH-002"),
        &String::from_str(&env, "WH2"), &admin);

    client.register_item(&owner, &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "Item A"), &ItemCategory::FinishedGoods,
        &String::from_str(&env, "WH-001"), &100u64,
        &String::from_str(&env, "units"), &String::from_str(&env, ""));

    client.register_item(&owner, &String::from_str(&env, "ITEM-002"),
        &String::from_str(&env, "Item B"), &ItemCategory::Equipment,
        &String::from_str(&env, "WH-001"), &50u64,
        &String::from_str(&env, "units"), &String::from_str(&env, ""));

    client.transfer_item(
        &owner, &owner2,
        &String::from_str(&env, "ITEM-001"),
        &String::from_str(&env, "WH-002"),
        &100u64,
        &String::from_str(&env, "TXF-001"),
        &String::from_str(&env, ""),
    );

    let (items, warehouses, transfers) = client.get_stats();
    assert_eq!(items, 2);
    assert_eq!(warehouses, 2);
    assert_eq!(transfers, 1);
}
