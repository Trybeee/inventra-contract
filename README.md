# Inventra — Contract Repo

> **Decentralized inventory management on Stellar Soroban**

Inventra brings inventory management on-chain. Businesses register stock items, assign them to warehouses, and transfer them between owners — all recorded permanently on Stellar with an immutable audit trail that no single party can alter. Every stock movement, adjustment, and flag is written as an on-chain event that anyone with the contract ID can independently verify.

This is **Repo 1 of 3** in the Inventra project:

| Repo | Description |
|------|-------------|
| `inventra-contract` ← you are here | Soroban smart contract (Rust) |
| `inventra-backend` | NestJS REST API + event poller |
| `inventra-frontend` | Next.js inventory dashboard |

---

## What the Contract Does

**Warehouse Registry** — Admin registers warehouses before any stock can be assigned. Each warehouse has a manager address and an active/inactive status. Deactivated warehouses cannot receive new stock transfers.

**Inventory Registration** — Any address can register an inventory item, becoming its initial owner. Each item has a category, unit of measurement, quantity, batch number, and a warehouse assignment. The item ID must be unique — typically a SKU or generated ID that matches the backend database record.

**Secure Transfers** — The item owner signs a transfer transaction that moves stock (fully or partially) to a new owner and warehouse. A `TransferRecord` is written permanently on-chain for every movement, building an immutable chain of custody. Partial transfers deduct from the source quantity while keeping the original owner in place.

**Stock Adjustments** — Owners or admins can update an item's quantity (for physical stock counts, write-offs, or restocking). Every adjustment writes a `StockAdjustment` record with the previous and new quantity and a reason. Adjusting to zero automatically marks the item as `Consumed`.

**Item Status Management** — Admin can flag items under audit investigation and unflag them when resolved. Owners or admins can mark items as consumed when stock is fully used. Flagged or consumed items are blocked from transfers.

**Ownership Verification** — Any third party can call `verify_ownership(item_id, address)` to confirm who owns a specific item without trusting either party's claims. This is the core verification primitive for supply chain disputes.

---

## Data Structures

### `InventoryItem`

```rust
pub struct InventoryItem {
    pub id:                   String,       // unique item ID / SKU
    pub name:                 String,
    pub category:             ItemCategory, // RawMaterial | FinishedGoods | Equipment | Packaging | Consumable | Other
    pub owner:                Address,      // current owner
    pub warehouse_id:         String,       // current warehouse
    pub quantity:             u64,          // current stock level
    pub unit:                 String,       // "kg", "units", "boxes", etc.
    pub status:               ItemStatus,   // Active | Transferred | Consumed | Flagged
    pub registered_at_ledger: u32,
    pub updated_at_ledger:    u32,
    pub batch_number:         String,       // optional lot/batch for traceability
}
```

### `Warehouse`

```rust
pub struct Warehouse {
    pub id:                   String,
    pub name:                 String,
    pub manager:              Address,
    pub is_active:            bool,
    pub item_count:           u32,
    pub registered_at_ledger: u32,
}
```

### `TransferRecord` (immutable)

```rust
pub struct TransferRecord {
    pub id:                    String,
    pub item_id:               String,
    pub from_owner:            Address,
    pub to_owner:              Address,
    pub from_warehouse:        String,
    pub to_warehouse:          String,
    pub quantity:              u64,
    pub note:                  String,
    pub transferred_at_ledger: u32,
}
```

### `StockAdjustment` (immutable)

```rust
pub struct StockAdjustment {
    pub id:                 String,
    pub item_id:            String,
    pub adjusted_by:        Address,
    pub previous_quantity:  u64,
    pub new_quantity:       u64,
    pub reason:             String,
    pub adjusted_at_ledger: u32,
}
```

---

## Item Status State Machine

```
Active ──── transfer_item() (full) ──→ Transferred
Active ──── consume_item()         ──→ Consumed
Active ──── flag_item()            ──→ Flagged
Active ──── adjust_stock(0)        ──→ Consumed
Flagged ─── unflag_item()          ──→ Active
```

---

## Contract Functions

### `init(admin, treasury)`
Initialises the contract. Called once by the deployer.

### Warehouse management

| Function | Caller | Description |
|---|---|---|
| `register_warehouse(admin, warehouse_id, name, manager)` | Admin | Register a new warehouse |
| `deactivate_warehouse(admin, warehouse_id)` | Admin | Block new transfers to this warehouse |
| `reactivate_warehouse(admin, warehouse_id)` | Admin | Restore warehouse to active |

### Inventory

| Function | Caller | Description |
|---|---|---|
| `register_item(owner, item_id, name, category, warehouse_id, quantity, unit, batch_number)` | Owner | Register new item on-chain |
| `transfer_item(from_owner, to_owner, item_id, to_warehouse_id, quantity, transfer_id, note)` | Owner | Transfer full or partial stock |
| `adjust_stock(caller, item_id, new_quantity, adjustment_id, reason)` | Owner or Admin | Update stock quantity |
| `consume_item(caller, item_id)` | Owner or Admin | Mark item as consumed |
| `flag_item(admin, item_id, reason)` | Admin | Flag for audit |
| `unflag_item(admin, item_id)` | Admin | Clear flag |

### Read-only queries

| Function | Returns |
|---|---|
| `get_item(item_id)` | Full `InventoryItem` struct |
| `get_warehouse(warehouse_id)` | Full `Warehouse` struct |
| `get_transfer(transfer_id)` | Full `TransferRecord` struct |
| `get_adjustment(adjustment_id)` | Full `StockAdjustment` struct |
| `verify_ownership(item_id, address)` | `bool` |
| `item_exists(item_id)` | `bool` |
| `get_stock_quantity(item_id)` | `u64` |
| `get_item_warehouse(item_id)` | `String` — current warehouse ID |
| `get_stats()` | `(u32, u32, u32)` — total items, warehouses, transfers |

---

## Events

| Event name | Payload | When |
|---|---|---|
| `warehouse_registered` | `warehouse_id` | Warehouse created |
| `warehouse_deactivated` | `warehouse_id` | Warehouse deactivated |
| `warehouse_reactivated` | `warehouse_id` | Warehouse reactivated |
| `item_registered` | `(item_id, owner, warehouse_id, quantity)` | Item registered |
| `item_transferred` | `(item_id, from_owner, to_owner, to_warehouse, quantity)` | Transfer executed |
| `stock_adjusted` | `(item_id, adjusted_by, previous_qty, new_qty)` | Stock updated |
| `item_flagged` | `(item_id, reason)` | Item flagged for audit |
| `item_unflagged` | `item_id` | Flag cleared |
| `item_consumed` | `(item_id, caller)` | Item consumed |
| `admin_transferred` | `new_admin` | Admin role transferred |

---

## Project Structure

```
inventra-contract/
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── README.md
└── contracts/
    └── inventra/
        ├── Cargo.toml
        ├── Makefile
        └── src/
            ├── lib.rs      ← Full contract logic
            └── test.rs     ← 18 unit tests
```

---

## Setup

```bash
# Rust + wasm32 target
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32v1-none

# Stellar CLI
cargo install --locked stellar-cli --features opt
```

## Running Tests

```bash
cargo test
cargo test -- --nocapture
```

Expected: **18 tests, all passing.**

## Build & Deploy

```bash
make build
make optimize
export STELLAR_ACCOUNT=my-account
make deploy-testnet
```

### Initialize after deployment

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source my-account \
  --network testnet \
  -- init \
  --admin <ADMIN_ADDRESS>
```

---

## License

MIT
