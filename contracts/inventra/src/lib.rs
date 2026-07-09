#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

// ============================================================
// DATA TYPES
// ============================================================

/// The status of an inventory item
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum ItemStatus {
    /// Item is active and available in the warehouse
    Active,
    /// Item has been transferred and is no longer with the original owner
    Transferred,
    /// Item has been consumed, sold, or disposed of
    Consumed,
    /// Item is flagged for audit or discrepancy investigation
    Flagged,
}

/// The category of an inventory item — helps with filtering and reporting
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ItemCategory {
    RawMaterial,
    FinishedGoods,
    Equipment,
    Packaging,
    Consumable,
    Other,
}

/// A single inventory item registered on-chain
#[contracttype]
#[derive(Clone)]
pub struct InventoryItem {
    /// Unique item ID — e.g. "ITEM-2026-001" or a SKU
    pub id: String,
    /// Human-readable name of the item
    pub name: String,
    /// Item category
    pub category: ItemCategory,
    /// Current owner's Stellar address
    pub owner: Address,
    /// ID of the warehouse this item is currently stored in
    pub warehouse_id: String,
    /// Current quantity in stock
    pub quantity: u64,
    /// Unit of measurement — e.g. "kg", "units", "boxes"
    pub unit: String,
    /// Item status
    pub status: ItemStatus,
    /// Ledger sequence when the item was first registered
    pub registered_at_ledger: u32,
    /// Ledger sequence of the last update
    pub updated_at_ledger: u32,
    /// Optional batch/lot number for traceability
    pub batch_number: String,
}

/// A warehouse registered on-chain
#[contracttype]
#[derive(Clone)]
pub struct Warehouse {
    /// Unique warehouse ID
    pub id: String,
    /// Warehouse name or location label
    pub name: String,
    /// Manager's Stellar address — can receive transfers to this warehouse
    pub manager: Address,
    /// Whether the warehouse is currently accepting stock
    pub is_active: bool,
    /// Total number of distinct items currently registered at this warehouse
    pub item_count: u32,
    /// Ledger when the warehouse was registered
    pub registered_at_ledger: u32,
}

/// An immutable transfer record — written on every stock movement
/// This builds the on-chain transaction history for any item
#[contracttype]
#[derive(Clone)]
pub struct TransferRecord {
    /// Unique transfer ID
    pub id: String,
    /// The item that was transferred
    pub item_id: String,
    /// Sender's Stellar address
    pub from_owner: Address,
    /// Recipient's Stellar address
    pub to_owner: Address,
    /// Source warehouse
    pub from_warehouse: String,
    /// Destination warehouse
    pub to_warehouse: String,
    /// Quantity transferred
    pub quantity: u64,
    /// Optional note about this transfer
    pub note: String,
    /// Ledger sequence when the transfer occurred
    pub transferred_at_ledger: u32,
}

/// A stock adjustment record — written whenever quantity is updated
#[contracttype]
#[derive(Clone)]
pub struct StockAdjustment {
    /// Unique adjustment ID
    pub id: String,
    /// The item adjusted
    pub item_id: String,
    /// Who made the adjustment
    pub adjusted_by: Address,
    /// Quantity before adjustment
    pub previous_quantity: u64,
    /// Quantity after adjustment
    pub new_quantity: u64,
    /// Reason for adjustment
    pub reason: String,
    /// Ledger when adjustment occurred
    pub adjusted_at_ledger: u32,
}

// ============================================================
// STORAGE KEYS
// ============================================================

#[contracttype]
pub enum DataKey {
    /// Inventory item by item ID
    Item(String),
    /// Warehouse by warehouse ID
    Warehouse(String),
    /// Transfer record by transfer ID
    Transfer(String),
    /// Stock adjustment record by adjustment ID
    Adjustment(String),
    /// Admin address
    Admin,
    /// Total item count (for stats)
    TotalItems,
    /// Total warehouse count
    TotalWarehouses,
    /// Total transfer count
    TotalTransfers,
}

// ============================================================
// CONTRACT
// ============================================================

#[contract]
pub struct InventraContract;

#[contractimpl]
impl InventraContract {

    // ----------------------------------------------------------
    // INIT
    // ----------------------------------------------------------

    /// Initialise the contract with an admin address.
    /// The admin can register warehouses and flag/unflag items.
    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalItems, &0u32);
        env.storage().instance().set(&DataKey::TotalWarehouses, &0u32);
        env.storage().instance().set(&DataKey::TotalTransfers, &0u32);
    }

    // ----------------------------------------------------------
    // WAREHOUSE MANAGEMENT
    // ----------------------------------------------------------

    /// Register a new warehouse on-chain.
    /// Only admin can register warehouses.
    ///
    /// # Arguments
    /// - `admin`        — must match stored admin
    /// - `warehouse_id` — unique warehouse identifier
    /// - `name`         — human-readable name or location
    /// - `manager`      — Stellar address of the warehouse manager
    pub fn register_warehouse(
        env: Env,
        admin: Address,
        warehouse_id: String,
        name: String,
        manager: Address,
    ) -> String {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if env
            .storage()
            .persistent()
            .has(&DataKey::Warehouse(warehouse_id.clone()))
        {
            panic!("warehouse already registered");
        }

        let warehouse = Warehouse {
            id: warehouse_id.clone(),
            name,
            manager,
            is_active: true,
            item_count: 0,
            registered_at_ledger: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Warehouse(warehouse_id.clone()), &warehouse);

        env.storage().persistent().extend_ttl(
            &DataKey::Warehouse(warehouse_id.clone()),
            100_000,
            6_300_000,
        );

        let total: u32 = env.storage().instance()
            .get(&DataKey::TotalWarehouses).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalWarehouses, &(total + 1));

        env.events().publish(
            (Symbol::new(&env, "warehouse_registered"), warehouse_id.clone()),
            warehouse_id.clone(),
        );

        warehouse_id
    }

    /// Deactivate a warehouse — prevents new stock from being transferred in.
    /// Existing items are unaffected.
    pub fn deactivate_warehouse(env: Env, admin: Address, warehouse_id: String) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut warehouse = Self::get_warehouse_internal(&env, &warehouse_id);
        warehouse.is_active = false;
        env.storage()
            .persistent()
            .set(&DataKey::Warehouse(warehouse_id.clone()), &warehouse);

        env.events().publish(
            (Symbol::new(&env, "warehouse_deactivated"), warehouse_id.clone()),
            warehouse_id,
        );
    }

    /// Reactivate a previously deactivated warehouse.
    pub fn reactivate_warehouse(env: Env, admin: Address, warehouse_id: String) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut warehouse = Self::get_warehouse_internal(&env, &warehouse_id);
        warehouse.is_active = true;
        env.storage()
            .persistent()
            .set(&DataKey::Warehouse(warehouse_id.clone()), &warehouse);

        env.events().publish(
            (Symbol::new(&env, "warehouse_reactivated"), warehouse_id.clone()),
            warehouse_id,
        );
    }

    // ----------------------------------------------------------
    // INVENTORY REGISTRATION
    // ----------------------------------------------------------

    /// Register a new inventory item on-chain.
    /// The registering address becomes the initial owner.
    ///
    /// # Arguments
    /// - `owner`        — owner's Stellar address (must sign)
    /// - `item_id`      — unique item identifier (e.g. SKU or generated ID)
    /// - `name`         — item name
    /// - `category`     — item category
    /// - `warehouse_id` — warehouse where item is initially stored
    /// - `quantity`     — initial quantity
    /// - `unit`         — unit of measurement (e.g. "kg", "units", "boxes")
    /// - `batch_number` — optional batch/lot number for traceability
    pub fn register_item(
        env: Env,
        owner: Address,
        item_id: String,
        name: String,
        category: ItemCategory,
        warehouse_id: String,
        quantity: u64,
        unit: String,
        batch_number: String,
    ) -> String {
        owner.require_auth();

        if quantity == 0 {
            panic!("quantity must be greater than zero");
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::Item(item_id.clone()))
        {
            panic!("item already registered");
        }

        // Verify warehouse exists and is active
        let mut warehouse = Self::get_warehouse_internal(&env, &warehouse_id);
        if !warehouse.is_active {
            panic!("warehouse is not active");
        }

        let current_ledger = env.ledger().sequence();

        let item = InventoryItem {
            id:                   item_id.clone(),
            name,
            category,
            owner:                owner.clone(),
            warehouse_id:         warehouse_id.clone(),
            quantity,
            unit,
            status:               ItemStatus::Active,
            registered_at_ledger: current_ledger,
            updated_at_ledger:    current_ledger,
            batch_number,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        env.storage().persistent().extend_ttl(
            &DataKey::Item(item_id.clone()),
            100_000,
            6_300_000,
        );

        // Increment warehouse item count
        warehouse.item_count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Warehouse(warehouse_id.clone()), &warehouse);

        // Increment total items
        let total: u32 = env.storage().instance()
            .get(&DataKey::TotalItems).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalItems, &(total + 1));

        env.events().publish(
            (Symbol::new(&env, "item_registered"), item_id.clone()),
            (owner, warehouse_id, quantity),
        );

        item_id
    }

    // ----------------------------------------------------------
    // STOCK TRANSFER
    // ----------------------------------------------------------

    /// Transfer an inventory item (or a portion of its stock) to a new owner and warehouse.
    ///
    /// The current owner must sign. The destination warehouse must be active.
    /// A partial transfer splits the item — the source retains the remaining quantity
    /// and a new TransferRecord is written to the immutable history.
    ///
    /// # Arguments
    /// - `from_owner`      — current owner (must sign)
    /// - `to_owner`        — recipient's Stellar address
    /// - `item_id`         — the item to transfer
    /// - `to_warehouse_id` — destination warehouse
    /// - `quantity`        — quantity to transfer (must be ≤ item quantity)
    /// - `transfer_id`     — unique ID for this transfer record
    /// - `note`            — optional note about this transfer
    pub fn transfer_item(
        env: Env,
        from_owner: Address,
        to_owner: Address,
        item_id: String,
        to_warehouse_id: String,
        quantity: u64,
        transfer_id: String,
        note: String,
    ) {
        from_owner.require_auth();

        if quantity == 0 {
            panic!("transfer quantity must be greater than zero");
        }

        // Ensure transfer ID is unique
        if env
            .storage()
            .persistent()
            .has(&DataKey::Transfer(transfer_id.clone()))
        {
            panic!("transfer ID already exists");
        }

        let mut item = Self::get_item_internal(&env, &item_id);

        if item.status != ItemStatus::Active {
            panic!("item is not active");
        }

        if item.owner != from_owner {
            panic!("unauthorized: caller is not the item owner");
        }

        if quantity > item.quantity {
            panic!("transfer quantity exceeds available stock");
        }

        // Verify destination warehouse exists and is active
        let dest_warehouse = Self::get_warehouse_internal(&env, &to_warehouse_id);
        if !dest_warehouse.is_active {
            panic!("destination warehouse is not active");
        }

        let from_warehouse_id = item.warehouse_id.clone();
        let current_ledger = env.ledger().sequence();

        // Write immutable transfer record
        let record = TransferRecord {
            id:                    transfer_id.clone(),
            item_id:               item_id.clone(),
            from_owner:            from_owner.clone(),
            to_owner:              to_owner.clone(),
            from_warehouse:        from_warehouse_id.clone(),
            to_warehouse:          to_warehouse_id.clone(),
            quantity,
            note,
            transferred_at_ledger: current_ledger,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Transfer(transfer_id.clone()), &record);

        env.storage().persistent().extend_ttl(
            &DataKey::Transfer(transfer_id.clone()),
            100_000,
            6_300_000,
        );

        // Update item: deduct quantity, update owner and warehouse if full transfer
        if quantity == item.quantity {
            // Full transfer — change ownership
            item.owner        = to_owner.clone();
            item.warehouse_id = to_warehouse_id.clone();
            item.status       = ItemStatus::Active;
        } else {
            // Partial transfer — deduct quantity from source
            item.quantity -= quantity;
        }

        item.updated_at_ledger = current_ledger;
        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        // Increment global transfer count
        let total: u32 = env.storage().instance()
            .get(&DataKey::TotalTransfers).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalTransfers, &(total + 1));

        env.events().publish(
            (Symbol::new(&env, "item_transferred"), item_id.clone()),
            (from_owner, to_owner, to_warehouse_id, quantity),
        );
    }

    // ----------------------------------------------------------
    // STOCK ADJUSTMENT
    // ----------------------------------------------------------

    /// Adjust the quantity of an item in stock.
    /// Used for physical stock counts, write-offs, or restocking.
    /// Only the item owner or admin can adjust stock.
    /// Every adjustment writes an immutable StockAdjustment record.
    ///
    /// # Arguments
    /// - `caller`        — item owner or admin (must sign)
    /// - `item_id`       — the item to adjust
    /// - `new_quantity`  — the updated quantity (can be 0 for write-off)
    /// - `adjustment_id` — unique ID for this adjustment record
    /// - `reason`        — why the adjustment is being made
    pub fn adjust_stock(
        env: Env,
        caller: Address,
        item_id: String,
        new_quantity: u64,
        adjustment_id: String,
        reason: String,
    ) {
        caller.require_auth();

        // Ensure adjustment ID is unique
        if env
            .storage()
            .persistent()
            .has(&DataKey::Adjustment(adjustment_id.clone()))
        {
            panic!("adjustment ID already exists");
        }

        let mut item = Self::get_item_internal(&env, &item_id);

        if item.status == ItemStatus::Consumed || item.status == ItemStatus::Transferred {
            panic!("cannot adjust a consumed or transferred item");
        }

        let is_owner = item.owner == caller;
        let is_admin = Self::is_admin(&env, &caller);

        if !is_owner && !is_admin {
            panic!("unauthorized: must be item owner or admin");
        }

        let previous_quantity = item.quantity;
        let current_ledger    = env.ledger().sequence();

        // Write immutable adjustment record
        let adjustment = StockAdjustment {
            id:                adjustment_id.clone(),
            item_id:           item_id.clone(),
            adjusted_by:       caller.clone(),
            previous_quantity,
            new_quantity,
            reason,
            adjusted_at_ledger: current_ledger,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Adjustment(adjustment_id.clone()), &adjustment);

        env.storage().persistent().extend_ttl(
            &DataKey::Adjustment(adjustment_id.clone()),
            100_000,
            6_300_000,
        );

        // Update item quantity and status
        item.quantity          = new_quantity;
        item.updated_at_ledger = current_ledger;

        if new_quantity == 0 {
            item.status = ItemStatus::Consumed;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        env.events().publish(
            (Symbol::new(&env, "stock_adjusted"), item_id.clone()),
            (caller, previous_quantity, new_quantity),
        );
    }

    // ----------------------------------------------------------
    // ITEM STATUS MANAGEMENT
    // ----------------------------------------------------------

    /// Admin flags an item for audit or discrepancy investigation.
    pub fn flag_item(env: Env, admin: Address, item_id: String, reason: String) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut item = Self::get_item_internal(&env, &item_id);
        item.status = ItemStatus::Flagged;
        item.updated_at_ledger = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        env.events().publish(
            (Symbol::new(&env, "item_flagged"), item_id.clone()),
            reason,
        );
    }

    /// Admin clears a flag and restores an item to Active status.
    pub fn unflag_item(env: Env, admin: Address, item_id: String) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut item = Self::get_item_internal(&env, &item_id);
        if item.status != ItemStatus::Flagged {
            panic!("item is not flagged");
        }
        item.status = ItemStatus::Active;
        item.updated_at_ledger = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        env.events().publish(
            (Symbol::new(&env, "item_unflagged"), item_id.clone()),
            item_id,
        );
    }

    /// Mark an item as consumed — permanently removes it from active inventory.
    /// Only the item owner or admin can mark an item consumed.
    pub fn consume_item(env: Env, caller: Address, item_id: String) {
        caller.require_auth();

        let mut item = Self::get_item_internal(&env, &item_id);

        let is_owner = item.owner == caller;
        let is_admin = Self::is_admin(&env, &caller);

        if !is_owner && !is_admin {
            panic!("unauthorized");
        }

        if item.status != ItemStatus::Active {
            panic!("only active items can be consumed");
        }

        item.status            = ItemStatus::Consumed;
        item.updated_at_ledger = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&DataKey::Item(item_id.clone()), &item);

        env.events().publish(
            (Symbol::new(&env, "item_consumed"), item_id.clone()),
            caller,
        );
    }

    // ----------------------------------------------------------
    // ADMIN MANAGEMENT
    // ----------------------------------------------------------

    /// Transfer admin role to a new address.
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"), new_admin.clone()),
            new_admin,
        );
    }

    // ----------------------------------------------------------
    // READ-ONLY QUERIES
    // ----------------------------------------------------------

    /// Get an inventory item by ID
    pub fn get_item(env: Env, item_id: String) -> InventoryItem {
        Self::get_item_internal(&env, &item_id)
    }

    /// Get a warehouse by ID
    pub fn get_warehouse(env: Env, warehouse_id: String) -> Warehouse {
        Self::get_warehouse_internal(&env, &warehouse_id)
    }

    /// Get a transfer record by ID
    pub fn get_transfer(env: Env, transfer_id: String) -> TransferRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .unwrap_or_else(|| panic!("transfer record not found"))
    }

    /// Get a stock adjustment record by ID
    pub fn get_adjustment(env: Env, adjustment_id: String) -> StockAdjustment {
        env.storage()
            .persistent()
            .get(&DataKey::Adjustment(adjustment_id))
            .unwrap_or_else(|| panic!("adjustment record not found"))
    }

    /// Check whether a given address is the current owner of an item
    pub fn verify_ownership(env: Env, item_id: String, address: Address) -> bool {
        if let Some(item) = env
            .storage()
            .persistent()
            .get::<DataKey, InventoryItem>(&DataKey::Item(item_id))
        {
            item.owner == address
        } else {
            false
        }
    }

    /// Check whether an item exists on-chain
    pub fn item_exists(env: Env, item_id: String) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Item(item_id))
    }

    /// Get the current stock quantity for an item
    pub fn get_stock_quantity(env: Env, item_id: String) -> u64 {
        let item = Self::get_item_internal(&env, &item_id);
        item.quantity
    }

    /// Get the current warehouse for an item
    pub fn get_item_warehouse(env: Env, item_id: String) -> String {
        let item = Self::get_item_internal(&env, &item_id);
        item.warehouse_id
    }

    /// Get platform-wide stats
    pub fn get_stats(env: Env) -> (u32, u32, u32) {
        let items:      u32 = env.storage().instance().get(&DataKey::TotalItems).unwrap_or(0);
        let warehouses: u32 = env.storage().instance().get(&DataKey::TotalWarehouses).unwrap_or(0);
        let transfers:  u32 = env.storage().instance().get(&DataKey::TotalTransfers).unwrap_or(0);
        (items, warehouses, transfers)
    }

    
}

mod test;
