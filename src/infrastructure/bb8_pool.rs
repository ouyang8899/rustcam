
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use rbdc::db::{ConnectOptions, Connection, Driver, ExecResult, Row};
use rbdc::error::Error;
use rbdc::pool::ConnectionManager;
use rbs::Value;

type Slot = Option<Box<dyn Connection>>;

struct Bb8ManageConnection {
    manager: ConnectionManager,
}

impl bb8::ManageConnection for Bb8ManageConnection {
    type Connection = Slot;
    type Error = Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {

        let conn = self
            .manager
            .driver
            .connect_opt(self.manager.option.deref().deref())
            .await?;
        Ok(Some(conn))
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        match conn {
            Some(c) => c.ping().await,
            None => Err(Error::from("[DB] empty connection slot")),
        }
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.is_none()
    }
}

pub struct PooledConn {
    pool: Arc<bb8::Pool<Bb8ManageConnection>>,
    conn: Option<Box<dyn Connection>>,
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {

            let _ = self.pool.add(Some(conn));
        }
    }
}

impl std::fmt::Debug for PooledConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledConn")
            .field("alive", &self.conn.is_some())
            .finish()
    }
}

impl Connection for PooledConn {
    fn exec_rows(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, Error>>, Error>> {
        match self.conn.as_mut() {
            Some(c) => c.as_mut().exec_rows(sql, params),
            None => Box::pin(async { Err(Error::from("[DB] connection already closed")) }),
        }
    }

    fn exec_decode(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<Value, Error>> {
        match self.conn.as_mut() {
            Some(c) => c.as_mut().exec_decode(sql, params),
            None => Box::pin(async { Err(Error::from("[DB] connection already closed")) }),
        }
    }

    fn exec(&mut self, sql: &str, params: Vec<Value>) -> BoxFuture<'_, Result<ExecResult, Error>> {
        match self.conn.as_mut() {
            Some(c) => c.as_mut().exec(sql, params),
            None => Box::pin(async { Err(Error::from("[DB] connection already closed")) }),
        }
    }

    fn ping(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        match self.conn.as_mut() {
            Some(c) => c.as_mut().ping(),
            None => Box::pin(async { Err(Error::from("[DB] connection already closed")) }),
        }
    }

    fn close(&mut self) -> BoxFuture<'_, Result<(), Error>> {

        let conn = self.conn.take();
        Box::pin(async move {
            match conn {
                Some(mut c) => c.as_mut().close().await,
                None => Ok(()),
            }
        })
    }
}

pub struct Bb8Pool {
    pool: Arc<bb8::Pool<Bb8ManageConnection>>,
    driver: Option<Arc<Box<dyn Driver>>>,
    option: Option<Arc<Box<dyn ConnectOptions>>>,
}

impl std::fmt::Debug for Bb8Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bb8Pool").finish()
    }
}

impl Bb8Pool {

    pub async fn new(
        driver: Arc<Box<dyn Driver>>,
        option: Arc<Box<dyn ConnectOptions>>,
        max_size: u32,
        connect_timeout: Duration,
    ) -> Result<Self, Error> {
        let manager = ConnectionManager {
            driver: driver.clone(),
            option: option.clone(),
        };

        let pool = bb8::Pool::builder()
            .max_size(max_size.max(1))
            .connection_timeout(connect_timeout)
            .idle_timeout(Some(Duration::from_secs(300)))

            .test_on_check_out(true)
            .build(Bb8ManageConnection { manager })
            .await
            .map_err(|e| Error::from(format!("[DB] failed to create bb8 pool: {}", e)))?;

        Ok(Self {
            pool: Arc::new(pool),
            driver: Some(driver),
            option: Some(option),
        })
    }

    async fn acquire(&self) -> Result<Box<dyn Connection>, Error> {
        let mut guard = self.pool.get().await.map_err(|e| {
            Error::from(format!("[DB] failed to get connection: {}", e))
        })?;

        let taken = std::mem::take(&mut *guard);
        match taken {
            Some(conn) => Ok(Box::new(PooledConn {
                pool: self.pool.clone(),
                conn: Some(conn),
            })),
            None => Err(Error::from("[DB] acquired an empty connection slot")),
        }
    }
}

#[async_trait::async_trait]
impl rbdc::pool::Pool for Bb8Pool {
    fn new(manager: ConnectionManager) -> Result<Self, Error>
    where
        Self: Sized,
    {

        let driver = manager.driver.clone();
        let option = manager.option.clone();
        let m = ConnectionManager {
            driver: driver.clone(),
            option: option.clone(),
        };
        let pool = bb8::Pool::builder()
            .max_size(32)
            .connection_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(300)))
            .test_on_check_out(true)
            .build_unchecked(Bb8ManageConnection { manager: m });
        Ok(Self {
            pool: Arc::new(pool),
            driver: Some(driver),
            option: Some(option),
        })
    }

    async fn get(&self) -> Result<Box<dyn Connection>, Error> {
        self.acquire().await
    }

    async fn get_timeout(&self, d: Duration) -> Result<Box<dyn Connection>, Error> {
        match tokio::time::timeout(d, self.acquire()).await {
            Ok(res) => res,
            Err(_) => Err(Error::from("[DB] connection acquire timed out")),
        }
    }

    async fn set_max_idle_conns(&self, _n: u64) {

    }

    async fn set_max_open_conns(&self, _n: u64) {

    }

    fn driver_type(&self) -> &str {
        self.driver
            .as_ref()
            .map(|d| d.as_ref().name())
            .unwrap_or("postgres")
    }

    fn driver(&self) -> &dyn Driver {
        self.driver
            .as_ref()
            .map(|d| d.as_ref())
            .expect("[DB] driver not available")
    }
}
