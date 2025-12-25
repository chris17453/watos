//! Transaction Layer
//!
//! Transactions are the core of Copy-on-Write operation.
//! All modifications happen within a transaction context.
//! Commit atomically updates the root pointer to make changes visible.

#[cfg(feature = "std")]
use std::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::freespace::PendingFree;

// ============================================================================
// TRANSACTION STATE
// ============================================================================

/// Transaction state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionState {
    /// Transaction is active, modifications can be made
    Active,
    /// Transaction is being committed
    Committing,
    /// Transaction was successfully committed
    Committed,
    /// Transaction was aborted
    Aborted,
}

// ============================================================================
// MODIFIED BLOCK
// ============================================================================

/// A block that was modified in this transaction
#[derive(Clone, Debug)]
pub struct ModifiedBlock {
    /// Original block number (0 if newly allocated)
    pub original_block: u64,
    /// New block number where modified data is written
    pub new_block: u64,
    /// Type of modification
    pub mod_type: ModificationType,
}

/// Type of block modification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModificationType {
    /// Block was newly allocated
    Allocated,
    /// Block was copied (CoW)
    Copied,
    /// Block content was modified in place (only if exclusive)
    Modified,
}

// ============================================================================
// TRANSACTION CONTEXT
// ============================================================================

/// Transaction context
///
/// Tracks all modifications made during a transaction.
/// On commit, these are flushed to disk and the root pointer is updated.
/// On abort, all allocated blocks are returned to free space.
#[derive(Clone, Debug)]
pub struct Transaction {
    /// Transaction ID (increments monotonically)
    pub id: u64,

    /// Current state
    pub state: TransactionState,

    /// Generation number for this transaction
    pub generation: u64,

    /// Working root block (private to this transaction)
    pub working_root: u64,

    /// Blocks modified in this transaction
    pub modified_blocks: Vec<ModifiedBlock>,

    /// Blocks allocated in this transaction
    pub allocated_blocks: Vec<u64>,

    /// Blocks pending free (will be freed after commit)
    pub pending_frees: Vec<PendingFree>,

    /// Inodes modified (for cache invalidation)
    pub modified_inodes: Vec<u64>,

    /// Dirty flag - has any modification been made?
    pub dirty: bool,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(id: u64, generation: u64, current_root: u64) -> Self {
        Self {
            id,
            state: TransactionState::Active,
            generation,
            working_root: current_root,
            modified_blocks: Vec::new(),
            allocated_blocks: Vec::new(),
            pending_frees: Vec::new(),
            modified_inodes: Vec::new(),
            dirty: false,
        }
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// Check if transaction is dirty (has modifications)
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Record a block allocation
    pub fn record_allocation(&mut self, block: u64) {
        self.allocated_blocks.push(block);
        self.dirty = true;
    }

    /// Record a CoW copy
    pub fn record_cow_copy(&mut self, original: u64, new_block: u64) {
        self.modified_blocks.push(ModifiedBlock {
            original_block: original,
            new_block,
            mod_type: ModificationType::Copied,
        });
        self.dirty = true;
    }

    /// Record a block modification
    pub fn record_modification(&mut self, original: u64, new_block: u64) {
        self.modified_blocks.push(ModifiedBlock {
            original_block: original,
            new_block,
            mod_type: ModificationType::Modified,
        });
        self.dirty = true;
    }

    /// Schedule a block to be freed after commit
    pub fn schedule_free(&mut self, block: u64, count: u64) {
        self.pending_frees.push(PendingFree::new(block, count, self.id));
        self.dirty = true;
    }

    /// Record an inode modification (for cache invalidation)
    pub fn record_inode_modification(&mut self, inode_num: u64) {
        if !self.modified_inodes.contains(&inode_num) {
            self.modified_inodes.push(inode_num);
        }
    }

    /// Update the working root (when CoW copies the root node)
    pub fn update_working_root(&mut self, new_root: u64) {
        self.working_root = new_root;
        self.dirty = true;
    }

    /// Begin commit process
    pub fn begin_commit(&mut self) -> bool {
        if self.state != TransactionState::Active {
            return false;
        }
        self.state = TransactionState::Committing;
        true
    }

    /// Complete commit
    pub fn complete_commit(&mut self) {
        self.state = TransactionState::Committed;
    }

    /// Abort transaction
    pub fn abort(&mut self) {
        self.state = TransactionState::Aborted;
    }

    /// Get blocks to free on abort
    ///
    /// If transaction is aborted, all allocated blocks should be freed.
    pub fn blocks_to_free_on_abort(&self) -> &[u64] {
        &self.allocated_blocks
    }

    /// Get pending frees to process on commit
    pub fn pending_frees_on_commit(&self) -> &[PendingFree] {
        &self.pending_frees
    }

    /// Count of allocated blocks
    pub fn allocated_count(&self) -> usize {
        self.allocated_blocks.len()
    }

    /// Count of modified blocks
    pub fn modified_count(&self) -> usize {
        self.modified_blocks.len()
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

// ============================================================================
// COMMIT RECORD
// ============================================================================

/// Record of a committed transaction (for recovery/debugging)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CommitRecord {
    /// Transaction ID
    pub transaction_id: u64,
    /// Generation after commit
    pub generation: u64,
    /// New root block
    pub root_block: u64,
    /// Number of blocks allocated
    pub blocks_allocated: u32,
    /// Number of blocks freed
    pub blocks_freed: u32,
    /// Timestamp of commit
    pub timestamp: u64,
}

impl CommitRecord {
    pub fn new(txn: &Transaction, timestamp: u64) -> Self {
        Self {
            transaction_id: txn.id,
            generation: txn.generation,
            root_block: txn.working_root,
            blocks_allocated: txn.allocated_blocks.len() as u32,
            blocks_freed: txn.pending_frees.len() as u32,
            timestamp,
        }
    }
}

// ============================================================================
// TRANSACTION MANAGER TRAIT
// ============================================================================

/// Trait for transaction management
///
/// Implemented by the filesystem to handle transaction lifecycle.
pub trait TransactionManager {
    /// Begin a new transaction
    fn begin_transaction(&mut self) -> Result<&mut Transaction, TransactionError>;

    /// Commit the current transaction
    fn commit_transaction(&mut self) -> Result<CommitRecord, TransactionError>;

    /// Abort the current transaction
    fn abort_transaction(&mut self) -> Result<(), TransactionError>;

    /// Get the current transaction (if any)
    fn current_transaction(&self) -> Option<&Transaction>;

    /// Get the current transaction mutably
    fn current_transaction_mut(&mut self) -> Option<&mut Transaction>;
}

/// Transaction errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// No active transaction
    NoActiveTransaction,
    /// Transaction already active
    TransactionAlreadyActive,
    /// Transaction in wrong state
    InvalidState,
    /// Disk I/O error
    IoError,
    /// Out of disk space
    NoSpace,
    /// Transaction too large
    TooLarge,
}
