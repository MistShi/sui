// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::Streamer;
use chrono::prelude::*;
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::convert::TryFrom;
use std::sync::Arc;
use sui_types::object::ObjectFormatOptions;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, error};

const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub fn get_unix_time_ms() -> u64 {
    let ts_ms = Utc::now().timestamp_millis();
    u64::try_from(ts_ms).expect("Travelling in time machine")
}

pub struct EventHandler {
    module_cache: SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    streamer: Streamer<EventEnvelope>,
}

impl EventHandler {
    pub fn new(validator_store: Arc<AuthorityStore>) -> Self {
        let streamer = Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE);
        Self {
            module_cache: SyncModuleCache::new(ResolverWrapper(validator_store)),
            streamer,
        }
    }

    pub async fn process_events(&self, effects: &TransactionEffects) {
        // serially dispatch event processing to honor events' orders.
        for event in &effects.events {
            if let Err(e) = self.process_event(event).await {
                error!(error =? e, "Failed to send EventEnvelope to dispatch");
            }
        }
    }

    pub async fn process_event(&self, event: &Event) -> SuiResult {
        let json_value = match event {
            Event::MoveEvent(event_obj) => {
                debug!(event =? event, "Process MoveEvent.");
                let move_struct = event_obj.to_move_struct_with_resolver(
                    ObjectFormatOptions::default(),
                    &self.module_cache,
                )?;
                Some(serde_json::to_value(&move_struct).map_err(|e| {
                    SuiError::ObjectSerializationError {
                        error: e.to_string(),
                    }
                })?)
            }
            _ => None,
        };
        let envelope = EventEnvelope::new(get_unix_time_ms(), None, event.clone(), json_value);
        // TODO store events here
        self.streamer.send(envelope).await
    }

    pub fn subscribe(&self) -> BroadcastStream<EventEnvelope> {
        self.streamer.subscribe()
    }
}
