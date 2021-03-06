//! Serves a new contract so is available for browsing.

use std::{fs::File, io::Read, path::PathBuf, sync::Arc};

use locutus_node::{libp2p::identity::ed25519::PublicKey, SqlitePool, WrappedState};
use locutus_runtime::{ContractCode, StateStore, WrappedContract};
use serde::Serialize;

const MAX_SIZE: i64 = 10 * 1024 * 1024;
const MAX_MEM_CACHE: u32 = 10_000_000;
const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

struct WebBundle {
    model_contract: WrappedContract<'static>,
    initial_state: WrappedState,
    view_contract: WrappedContract<'static>,
    view_content: WrappedState,
}

fn test_web(public_key: PublicKey) -> Result<WebBundle, std::io::Error> {
    fn get_model_contract(
        _public_key: PublicKey,
    ) -> std::io::Result<(WrappedContract<'static>, WrappedState)> {
        let path = PathBuf::from(CRATE_DIR).join("examples/freenet_microblogging_model.wasm");
        let mut bytes = Vec::new();
        File::open(path)?.read_to_end(&mut bytes)?;

        #[derive(Serialize)]
        struct Verification {
            public_key: Vec<u8>,
        }
        let params = serde_json::to_vec(&Verification { public_key: vec![] }).unwrap();
        let contract = WrappedContract::new(Arc::new(ContractCode::from(bytes)), params.into());

        let path = PathBuf::from(CRATE_DIR).join("examples/freenet_microblogging_model");
        let mut bytes = Vec::new();
        File::open(path)?.read_to_end(&mut bytes)?;

        Ok((contract, bytes.into()))
    }

    fn get_view_contract() -> std::io::Result<(WrappedContract<'static>, WrappedState)> {
        let path = PathBuf::from(CRATE_DIR).join("examples/freenet_microblogging_view.wasm");
        let mut bytes = Vec::new();
        File::open(path)?.read_to_end(&mut bytes)?;

        let contract =
            WrappedContract::new(Arc::new(ContractCode::from(bytes)), [].as_ref().into());

        let path = PathBuf::from(CRATE_DIR).join("examples/freenet_microblogging_view");
        let mut bytes = Vec::new();
        File::open(path)?.read_to_end(&mut bytes)?;

        Ok((contract, bytes.into()))
    }

    let (model_contract, initial_state) = get_model_contract(public_key)?;
    let (view_contract, view_content) = get_view_contract()?;

    Ok(WebBundle {
        model_contract,
        initial_state,
        view_contract,
        view_content,
    })
}

#[cfg(feature = "local")]
async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use http_gw::HttpGateway;
    use locutus_dev::ContractStore;
    use locutus_node::libp2p::identity::ed25519::Keypair;

    let keypair = Keypair::generate();
    let bundle = test_web(keypair.public())?;
    log::info!(
        "loading view contract {} in local node",
        bundle.view_contract.key().encode()
    );
    log::info!(
        "loading model contract {} in local node",
        bundle.model_contract.key().encode()
    );

    let tmp_path = std::env::temp_dir().join("locutus");
    let contract_store = ContractStore::new(tmp_path.join("contracts"), MAX_SIZE);
    let state_store = StateStore::new(SqlitePool::new().await?, MAX_MEM_CACHE).unwrap();
    let mut local_node =
        locutus_dev::LocalNode::new(contract_store.clone(), state_store.clone(), || {
            locutus_dev::set_cleanup_on_exit().unwrap();
        })
        .await?;
    let id = HttpGateway::next_client_id();
    local_node
        .preload(id, bundle.model_contract, bundle.initial_state)
        .await;
    local_node
        .preload(id, bundle.view_contract, bundle.view_content)
        .await;
    http_gw::local_node::run_local_node(local_node).await
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(not(feature = "local"))]
    {
        panic!("only allowed if local feature is enabled");
    }
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    // env_logger::Builder::from_default_env()
    //     .format_module_path(true)
    //     .filter_level(log::LevelFilter::Info)
    //     .init();

    #[allow(unused_variables)]
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    #[cfg(feature = "local")]
    {
        rt.block_on(run())?;
    }

    Ok(())
}
