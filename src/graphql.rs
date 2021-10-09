use std::sync::Arc;

use crate::{
    analysis::{
        gamma_exposure::{gamma_exposure, gamma_exposure_aggregate},
        option_stats::option_stats,
    },
    data_apis::tda,
    db::{self, FileDb},
    types::{stats::StrikeStats, GammaExposureStats, Ohlc, OhlcInterval, Quote},
};
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object};
use tokio::sync::Mutex;

pub type Schema = async_graphql::Schema<Root, EmptyMutation, EmptySubscription>;

pub fn schema(db: Arc<Mutex<FileDb>>) -> Schema {
    async_graphql::Schema::build(Root, EmptyMutation, EmptySubscription)
        .data(db)
        .finish()
}

pub struct Root;

#[Object]
impl Root {
    async fn quote(&self, symbol: String, token: Option<String>) -> anyhow::Result<Quote> {
        log::info!("Querying quote");
        let quote = tda::get_quote(&symbol, token).await.map_err(log_error)?;
        Ok(quote)
    }

    async fn ohlc(
        &self,
        symbol: String,
        #[graphql(default_with = "default_interval()")] interval: OhlcInterval,
        token: Option<String>,
    ) -> anyhow::Result<Vec<Ohlc>> {
        log::info!("Querying ohlc");
        let ohlc = tda::get_ohlc(&symbol, interval, token)
            .await
            .map_err(log_error)?;
        Ok(ohlc)
    }

    async fn symbols(&self, context: &Context<'_>) -> anyhow::Result<Vec<String>> {
        log::info!("Querying symbols");
        let db = context
            .data::<Arc<Mutex<FileDb>>>()
            .map_err(|_| anyhow::anyhow!("Failed to load db"))?;
        let db = db.lock().await;
        Ok(db.symbols())
    }

    async fn option_stats(
        &self,
        context: &Context<'_>,
        symbol: String,
        token: Option<String>,
    ) -> anyhow::Result<Vec<StrikeStats>> {
        log::info!("Querying option stats");
        let db = context
            .data::<Arc<Mutex<FileDb>>>()
            .map_err(|_| anyhow::anyhow!("Failed to load db"))?;
        let option_chain = db::option_chain(&symbol, db.clone(), token)
            .await
            .map_err(log_error)?;
        let stats = option_stats(&option_chain);
        Ok(stats)
    }

    async fn gamma_exposure(
        &self,
        context: &Context<'_>,
        symbol: String,
        token: Option<String>,
    ) -> anyhow::Result<GammaExposureStats> {
        log::info!("Querying gamma exposure");
        let db = context
            .data::<Arc<Mutex<FileDb>>>()
            .map_err(|_| anyhow::anyhow!("Failed to load db"))?;
        let option_chain = db::option_chain(&symbol, db.clone(), token)
            .await
            .map_err(log_error)?;
        let gex = gamma_exposure(&symbol, &option_chain).unwrap();
        Ok(gex)
    }

    async fn gamma_exposure_aggregate(
        &self,
        context: &Context<'_>,
        symbol: String,
        token: Option<String>,
    ) -> anyhow::Result<GammaExposureStats> {
        log::info!("Querying gamma exposure aggregate");
        let db = context
            .data::<Arc<Mutex<FileDb>>>()
            .map_err(|_| anyhow::anyhow!("Failed to load db"))?;
        let option_chain = db::option_chain(&symbol, db.clone(), token)
            .await
            .map_err(log_error)?;
        let gex_agg = gamma_exposure_aggregate(&symbol, &option_chain).unwrap();
        Ok(gex_agg)
    }
}

fn default_interval() -> OhlcInterval {
    OhlcInterval::FiveMinute
}

fn log_error(error: anyhow::Error) -> anyhow::Error {
    log::error!("{}", error);
    error
}
