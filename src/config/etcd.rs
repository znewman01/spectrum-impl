use crate::config::store::{Error, Key, Store, Value};
use derivative::Derivative;
use etcd_rs::{Client, ClientConfig, KeyRange, PutRequest, RangeRequest};
use tonic::async_trait;

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub(in crate::config) struct EtcdStore {
    #[allow(dead_code)] // specifically for Debug
    endpoints: Vec<String>,
    #[derivative(Debug = "ignore")]
    client: Client, // etcd_rs::Client is thread-safe/cheaply cloneable
}

impl EtcdStore {
    #[allow(dead_code)]
    pub async fn connect(endpoint: String) -> Result<EtcdStore, Box<dyn std::error::Error>> {
        let endpoints = vec![endpoint];
        let client = Client::connect(ClientConfig {
            endpoints: endpoints.clone(),
            auth: None,
        })
        .await?;
        Ok(EtcdStore { endpoints, client })
    }
}

#[async_trait]
impl Store for EtcdStore {
    async fn get(&self, key: Key) -> Result<Option<Value>, Error> {
        let key = key.join("/");
        let range = KeyRange::key(key);
        let mut response = self
            .client
            .kv()
            .range(RangeRequest::new(range))
            .await
            .map_err(|e| e.to_string())
            .unwrap();
        Ok(response
            .take_kvs()
            .first()
            .map(|kv| kv.value_str().to_string()))
    }

    async fn put(&self, key: Key, value: Value) -> Result<(), Error> {
        let key = key.join("/");
        self.client
            .kv()
            .put(PutRequest::new(key, value))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, Error> {
        let prefix = prefix.join("/") + "/";
        let range = KeyRange::prefix(prefix);
        let mut response = self
            .client
            .kv()
            .range(RangeRequest::new(range))
            .await
            .map_err(|e| e.to_string())?;
        Ok(response
            .take_kvs()
            .into_iter()
            .map(|kv| {
                (
                    kv.key_str()
                        .to_string()
                        .split('/')
                        .map(ToString::to_string)
                        .collect(),
                    kv.value_str().to_string(),
                )
            })
            .collect())
    }
}

#[cfg(all(test, feature = "etcd-tests"))]
mod tests {
    use super::*;
    use crate::config::store::tests::*;

    use etcd_rs::DeleteRequest;
    use port_check::free_local_port;
    use proptest::collection::hash_set;
    use proptest::test_runner::TestRunner;
    use std::ffi::OsStr;
    use std::process::Stdio;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::{
        process::{Child, Command},
        time::delay_for,
    };

    /// Clear the etcd store between test runs.
    async fn clear(client: Client) -> Result<(), Error> {
        client
            .kv()
            .delete(DeleteRequest::new(KeyRange::all()))
            .await
            .map_err(|e| e.to_string())
            .unwrap();
        Ok(())
    }

    #[derive(Debug)]
    struct TestEtcdStore {
        client_addr: String,
        temp_dir: TempDir,
        process: Child,
    }

    fn get_addr() -> String {
        let port = free_local_port().expect("No ports free.");
        format!("http://127.0.0.1:{}", port)
    }

    impl TestEtcdStore {
        async fn create() -> Result<Self, Box<dyn std::error::Error>> {
            let temp_dir = tempfile::tempdir().expect("Couldn't create temp dir");
            let data_dir = temp_dir.path().join("test.etcd");
            let data_dir: &OsStr = data_dir.as_ref();

            let client_addr = get_addr();
            let peer_addr = get_addr();

            let process = Command::new("etcd")
                .arg("--data-dir")
                .arg(data_dir)
                .arg("--listen-client-urls")
                .arg(client_addr.clone())
                .arg("--advertise-client-urls")
                .arg(client_addr.clone())
                .arg("--listen-peer-urls")
                .arg(peer_addr.clone())
                .kill_on_drop(true)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("etcd failed to spawn");

            // wait for etcd to start up
            delay_for(Duration::from_millis(50)).await;

            Ok(TestEtcdStore {
                client_addr,
                temp_dir,
                process,
            })
        }

        async fn store(&self) -> Result<EtcdStore, Box<dyn std::error::Error>> {
            Ok(EtcdStore::connect(self.client_addr.clone()).await?)
        }
    }

    // The below is a little bit of a hack.
    // Two problems:
    // 1) The usual proptest-tokio::test incompatibility
    // 2) Starting up an etcd process for each proptest case is a pain in the butt.
    //    Instead, we start one up per test, and run all the cases with it.

    #[tokio::test(threaded_scheduler)]
    async fn test_put_and_get() {
        let wrapper = TestEtcdStore::create().await.unwrap();
        let store = wrapper.store().await.unwrap();

        TestRunner::default()
            .run(&(keys(), values()), |(key, value)| {
                futures::executor::block_on(async {
                    clear(store.client.clone()).await?;
                    run_test_put_and_get(store.clone(), key, value).await
                })
            })
            .unwrap()
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_get_empty() {
        let wrapper = TestEtcdStore::create().await.unwrap();
        let store = wrapper.store().await.unwrap();

        TestRunner::default()
            .run(&keys(), |key| {
                futures::executor::block_on(async {
                    clear(store.client.clone()).await?;
                    run_test_get_empty(store.clone(), key).await
                })
            })
            .unwrap()
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_put_and_get_keep_latter() {
        let wrapper = TestEtcdStore::create().await.unwrap();
        let store = wrapper.store().await.unwrap();

        TestRunner::default()
            .run(&(keys(), values(), values()), |(key, value1, value2)| {
                futures::executor::block_on(async {
                    clear(store.client.clone()).await?;
                    run_test_put_and_get_keep_latter(store.clone(), key, value1, value2).await
                })
            })
            .unwrap()
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_list() {
        let wrapper = TestEtcdStore::create().await.unwrap();
        let store = wrapper.store().await.unwrap();

        TestRunner::default()
            .run(
                &(
                    keys(),
                    hash_set(keys(), 0..10usize),
                    hash_set(keys(), 0..10usize),
                    values(),
                ),
                |(prefix, suffixes, other_keys, value)| {
                    futures::executor::block_on(async {
                        clear(store.client.clone()).await?;
                        run_test_list(store.clone(), prefix, suffixes, other_keys, value).await
                    })
                },
            )
            .unwrap()
    }
}
