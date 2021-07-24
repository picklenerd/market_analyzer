use std::{
    collections::{BTreeMap, HashSet},
    convert::TryFrom,
};

use chrono::{Date, Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::{data_apis::tradier, math::bs::gamma};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GammaExposureStats {
    pub prices: Vec<GammaExposure>,
    pub average_absolute_exposure: f64,
    pub average_positive_exposure: f64,
    pub average_negative_exposure: f64,
    pub maximum: f64,
    pub minimum: f64,
    pub absolute_maximum: f64,
    pub weighted_average_absolute_price: f64,
    pub weighted_average_positive_price: f64,
    pub weighted_average_negative_price: f64,
}

impl GammaExposureStats {
    pub fn new(strike_to_gamma_exposure: &BTreeMap<String, f64>) -> anyhow::Result<Self> {
        let mut positive_sum: f64 = 0.0;
        let mut positive_count = 0;
        let mut negative_sum: f64 = 0.0;
        let mut negative_count = 0;
        let mut maximum: f64 = 0.0;
        let mut minimum: f64 = 0.0;
        let mut absolute_maximum: f64 = 0.0;
        let mut weighted_positive_sum: f64 = 0.0;
        let mut weighted_negative_sum: f64 = 0.0;

        for (strike, exposure) in strike_to_gamma_exposure {
            let strike: f64 = strike.parse()?;
            if *exposure >= 0.0 {
                positive_sum += exposure;
                weighted_positive_sum += strike * exposure;
                positive_count += 1;
            } else {
                negative_sum += exposure;
                weighted_negative_sum += strike * *exposure;
                negative_count += 1;
            }
            maximum = maximum.max(*exposure);
            minimum = minimum.min(*exposure);
            absolute_maximum = absolute_maximum.max(exposure.abs());
        }

        positive_count = positive_count.max(1);
        negative_count = negative_count.max(1);

        let average_positive_exposure = positive_sum / positive_count as f64;
        let average_negative_exposure = negative_sum / negative_count as f64;
        let average_absolute_exposure =
            (positive_sum.abs() + negative_sum.abs()) / (positive_count + negative_count) as f64;

        let weighted_average_absolute_price =
            f64::try_from(weighted_positive_sum.abs() + weighted_negative_sum.abs())?
                / (positive_sum.abs() + negative_sum.abs());
        let weighted_average_positive_price = f64::try_from(weighted_positive_sum)? / positive_sum;
        let weighted_average_negative_price = f64::try_from(weighted_negative_sum)? / negative_sum;

        let mut prices: Vec<GammaExposure> = strike_to_gamma_exposure
            .iter()
            .map(|(strike, exposure)| GammaExposure::new(strike.clone(), *exposure))
            .collect();

        prices.sort_by(|p1, p2| p1.strike.cmp(&p2.strike));

        Ok(Self {
            prices,
            average_absolute_exposure,
            average_positive_exposure,
            average_negative_exposure,
            maximum,
            minimum,
            absolute_maximum,
            weighted_average_absolute_price,
            weighted_average_positive_price,
            weighted_average_negative_price,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GammaExposure {
    pub strike: String,
    pub gamma_exposure: f64,
}

impl GammaExposure {
    pub fn new(strike: String, gamma_exposure: f64) -> Self {
        Self {
            strike,
            gamma_exposure,
        }
    }
}

pub async fn gamma_exposure_by_price(
    symbol: &str,
    force_download: bool,
) -> anyhow::Result<BTreeMap<String, f64>> {
    let options = tradier::get_option_chain(&symbol.to_uppercase(), force_download).await?;

    let mut strike_to_gamma_exposure: BTreeMap<String, f64> = BTreeMap::new();

    for option in options {
        let strike = option.strike.to_string();
        if let Some(greeks) = option.greeks {
            let mut exposure = if greeks.gamma > 1.0 || greeks.gamma < -1.0 {
                0.0
            } else {
                greeks.gamma * option.open_interest as f64
            };
            if option.option_type == "put" {
                exposure *= -1.0;
            }
            match strike_to_gamma_exposure.get_mut(&strike) {
                Some(exp) => *exp += exposure,
                None => {
                    strike_to_gamma_exposure.insert(strike, exposure);
                }
            }
        }
    }

    Ok(strike_to_gamma_exposure)
}

pub async fn gamma_exposure_stats(
    symbol: &str,
    force_download: bool,
) -> anyhow::Result<GammaExposureStats> {
    let strike_to_gamma_exposure = gamma_exposure_by_price(symbol, force_download).await?;
    Ok(GammaExposureStats::new(&strike_to_gamma_exposure)?)
}

pub async fn gamma_exposure_aggregate(
    symbol: &str,
    force_download: bool,
) -> anyhow::Result<GammaExposureStats> {
    let options = tradier::get_option_chain(&symbol.to_uppercase(), force_download).await?;

    let now = Local::now().date();
    let mut strike_to_gamma_exposure_aggregate: BTreeMap<String, f64> = BTreeMap::new();

    let strikes: Vec<f64> = options
        .iter()
        .map(|o| (o.strike * 100.0) as u64)
        .collect::<HashSet<u64>>()
        .into_iter()
        .map(|s| s as f64 / 100.0)
        .collect();

    for option in options {
        let expiration_date = parse_date(&option.expiration_date)?;
        let days_remaining = expiration_date.signed_duration_since(now).num_days();

        let sigma = option.greeks.map(|g| g.mid_iv).unwrap_or(0.0);
        let expiration_time = days_remaining as f64 / 365.0;
        let current_time = 0.0;
        let strike = option.strike;

        let strike_string = option.strike.to_string();

        for price in &strikes {
            let gamma = gamma(sigma, expiration_time, current_time, *price, strike);

            let mut exposure = if gamma > 1.0 || gamma < -1.0  || gamma.is_nan() {
                0.0
            } else {
                gamma * option.open_interest as f64
            };
            if option.option_type == "put" {
                exposure *= -1.0;
            }
            match strike_to_gamma_exposure_aggregate.get_mut(&strike_string) {
                Some(exp) => *exp += exposure,
                None => {
                    strike_to_gamma_exposure_aggregate.insert(strike_string.clone(), exposure);
                }
            }
        }
    }

    // dbg!(&strike_to_gamma_exposure_aggregate);

    Ok(GammaExposureStats::new(&strike_to_gamma_exposure_aggregate)?)
}

fn parse_date(date: &str) -> anyhow::Result<Date<Local>> {
    let mut split_date = date.split("-");

    let y: i32 = split_date
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid year"))?
        .parse()?;
    let m: u32 = split_date
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid month"))?
        .parse()?;
    let d: u32 = split_date
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid day"))?
        .parse()?;

    Ok(Local.ymd(y, m, d))
}