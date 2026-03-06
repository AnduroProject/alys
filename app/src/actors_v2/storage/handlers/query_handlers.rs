//! Query-related message handlers for Storage Actor - V2

use crate::actors_v2::storage::{
    actor::{BlockRef, StorageActor, StorageError},
    cache::CacheStats,
    messages::*,
};
use actix::prelude::*;
use tracing::*;

impl Handler<GetChainHeadMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<BlockRef>, StorageError>>;

    fn handle(&mut self, msg: GetChainHeadMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetChainHeadMessage");

        let database = self.database.clone();

        Box::pin(async move { database.get_chain_head().await })
    }
}

impl Handler<GetChainHeightMessage> for StorageActor {
    type Result = ResponseFuture<Result<u64, StorageError>>;

    fn handle(&mut self, msg: GetChainHeightMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetChainHeightMessage");

        let database = self.database.clone();

        Box::pin(async move {
            match database.get_chain_head().await? {
                Some(head) => Ok(head.number),
                None => Ok(0), // No chain head means genesis (height 0)
            }
        })
    }
}

impl Handler<UpdateChainHeadMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateChainHeadMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        info!(
            "Handling UpdateChainHeadMessage to {} at height {}",
            msg.new_head.hash, msg.new_head.number
        );

        let new_head = msg.new_head;
        let database = self.database.clone();
        let metrics = self.metrics.clone();

        Box::pin(async move {
            database.put_chain_head(&new_head).await?;
            metrics.record_chain_head_update();
            Ok(())
        })
    }
}

impl Handler<GetStatsMessage> for StorageActor {
    type Result = ResponseFuture<StorageStats>;

    fn handle(&mut self, msg: GetStatsMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetStatsMessage");

        let cache = self.cache.clone();
        let database = self.database.clone();
        let metrics = self.metrics.clone();
        let pending_writes_count = self.get_pending_writes_count();

        Box::pin(async move {
            let cache_stats = cache.get_stats().await;
            let hit_rates = cache.get_hit_rates().await;

            // Use accessor methods for atomic counter values
            let blocks_stored = metrics.get_blocks_stored();
            let state_updates = metrics.state_updates.load(std::sync::atomic::Ordering::Relaxed);

            let db_stats = match database.get_stats().await {
                Ok(stats) => stats,
                Err(e) => {
                    error!("Failed to get database stats: {}", e);
                    return StorageStats {
                        blocks_stored,
                        blocks_cached: 0,
                        state_entries: state_updates,
                        state_cached: 0,
                        cache_hit_rate: 0.0,
                        pending_writes: pending_writes_count as u64,
                        database_size_mb: 0,
                    };
                }
            };

            StorageStats {
                blocks_stored,
                blocks_cached: cache_stats.block_cache_bytes / 256, // Rough estimate
                state_entries: state_updates,
                state_cached: cache_stats.state_cache_bytes / 64, // Rough estimate
                cache_hit_rate: hit_rates.get("overall").copied().unwrap_or(0.0),
                pending_writes: pending_writes_count as u64,
                database_size_mb: db_stats.total_size_bytes / (1024 * 1024),
            }
        })
    }
}

impl Handler<GetCacheStatsMessage> for StorageActor {
    type Result = ResponseFuture<CacheStats>;

    fn handle(&mut self, msg: GetCacheStatsMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetCacheStatsMessage");

        let cache = self.cache.clone();

        Box::pin(async move { cache.get_stats().await })
    }
}

impl Handler<GetTransactionByHashMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<TransactionWithBlockInfo>, StorageError>>;

    fn handle(&mut self, msg: GetTransactionByHashMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling GetTransactionByHashMessage for tx {}",
            msg.tx_hash
        );

        let tx_hash = msg.tx_hash;
        let indexing = self.indexing.clone();

        Box::pin(async move {
            match indexing.read().await.get_transaction(&tx_hash).await? {
                Some(tx_index) => {
                    let tx_info = TransactionWithBlockInfo {
                        transaction_hash: tx_index.transaction_hash,
                        block_hash: tx_index.block_hash,
                        block_number: tx_index.block_number,
                        transaction_index: tx_index.transaction_index,
                    };
                    Ok(Some(tx_info))
                }
                None => Ok(None),
            }
        })
    }
}

impl Handler<GetAddressTransactionsMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<AddressTransactionInfo>, StorageError>>;

    fn handle(
        &mut self,
        msg: GetAddressTransactionsMessage,
        _: &mut Context<Self>,
    ) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling GetAddressTransactionsMessage for address {:?}",
            msg.address
        );

        let address = msg.address;
        let limit = msg.limit;
        let indexing = self.indexing.clone();

        Box::pin(async move {
            let address_indices = indexing
                .read()
                .await
                .get_address_transactions(&address, limit)
                .await?;

            let tx_info: Vec<AddressTransactionInfo> = address_indices
                .into_iter()
                .map(|addr_index| AddressTransactionInfo {
                    transaction_hash: addr_index.transaction_hash,
                    block_number: addr_index.block_number,
                    value: addr_index.value,
                    is_sender: addr_index.is_sender,
                    transaction_type: "transfer".to_string(), // Placeholder
                })
                .collect();

            Ok(tx_info)
        })
    }
}

impl Handler<QueryLogsMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<EventLog>, StorageError>>;

    fn handle(&mut self, msg: QueryLogsMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling QueryLogsMessage with filter from {:?} to {:?}",
            msg.filter.from_block, msg.filter.to_block
        );

        let filter = msg.filter;
        let indexing = self.indexing.clone();

        Box::pin(async move {
            let addresses = if let Some(addr) = filter.address {
                vec![addr]
            } else {
                vec![]
            };

            let eth_logs = indexing
                .read()
                .await
                .query_logs(
                    filter.from_block,
                    filter.to_block,
                    &addresses,
                    &filter.topics,
                )
                .await?;

            // Convert EthereumLog to EventLog
            let event_logs: Vec<EventLog> = eth_logs
                .into_iter()
                .map(|eth_log| EventLog {
                    address: eth_log.address,
                    topics: eth_log.topics,
                    data: eth_log.data,
                    block_hash: eth_log.block_hash,
                    block_number: eth_log.block_number,
                    transaction_hash: eth_log.transaction_hash,
                    log_index: eth_log.log_index,
                })
                .collect();

            Ok(event_logs)
        })
    }
}
