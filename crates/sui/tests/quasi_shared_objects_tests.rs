// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_config::ValidatorInfo;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::messages::CallArg;
use sui_types::object::Object;
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::move_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::transaction::{
    publish_package, submit_shared_object_transaction, submit_single_owner_transaction,
};

async fn publish_move_test_package(gas_object: Object, configs: &[ValidatorInfo]) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(gas_object, path, configs).await
}

#[tokio::test]
async fn normal_quasi_shared_object_flow() {
    // This test exercises a simple flow of quasi shared objects:
    // Create a shared object and a child of it, submit two transactions that both try to mutate the
    // child object (and specify the child object as QuasiSharedObject in the tx input).
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let package_ref =
        publish_move_test_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "create_shared_parent_and_child",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(transaction, configs.validator_set()).await;
    assert!(effects.status.is_ok());
    let ((parent_id, _, _), _) = *effects
        .created
        .iter()
        .find(|(_, owner)| owner.is_shared())
        .unwrap();
    let ((child_id, _, _), _) = *effects
        .created
        .iter()
        .find(|(_, owner)| !owner.is_shared())
        .unwrap();
    let tx1 = move_transaction(
        gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_counter",
        package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            CallArg::QuasiSharedObject(child_id),
        ],
    );
    let effects = submit_shared_object_transaction(tx1, configs.validator_set())
        .await
        .unwrap();
    assert!(effects.status.is_ok());

    let tx2 = move_transaction(
        gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_counter",
        package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            CallArg::QuasiSharedObject(child_id),
        ],
    );
    let effects = submit_shared_object_transaction(tx2, configs.validator_set())
        .await
        .unwrap();
    assert!(effects.status.is_ok());
    assert_eq!(
        effects
            .mutated
            .iter()
            .find(|((id, _, _), _)| id == &child_id)
            .unwrap()
            .0
             .1,
        SequenceNumber::from(3)
    );
}
