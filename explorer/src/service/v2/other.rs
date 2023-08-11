use crate::service::api::Api;
use crate::service::v1::chain::{AddressCount, AddressCountResponse, AddressCountResult};
use anyhow::Result;
use poem_openapi::param::Query;
use poem_openapi::{payload::Json, ApiResponse, Object};
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::Local;
use sqlx::Row;
use std::ops::Add;

#[derive(ApiResponse)]
pub enum V2ChainStatisticsResponse {
    #[oai(status = 200)]
    Ok(Json<V2ChainStatisticsResult>),
    #[oai(status = 404)]
    NotFound(Json<V2ChainStatisticsResult>),
    #[oai(status = 500)]
    InternalError(Json<V2ChainStatisticsResult>),
}

#[derive(Serialize, Deserialize, Object)]
pub struct V2ChainStatisticsResult {
    pub code: i32,
    pub message: String,
    pub data: Option<V2StatisticsData>,
}

#[derive(Serialize, Deserialize, Object)]
pub struct V2StatisticsData {
    pub active_addrs: i64,
    pub total_txs: i64,
    pub daily_txs: i64,
}

pub async fn v2_statistics(api: &Api) -> Result<V2ChainStatisticsResponse> {
    let mut conn = api.storage.lock().await.acquire().await?;

    // total txs
    let sql_txs_count = "select count(*) as cnt from transaction".to_string();
    let row = sqlx::query(sql_txs_count.as_str())
        .fetch_one(&mut conn)
        .await?;
    let total_txs = row.try_get("cnt")?;

    // total addrs
    let sql_addr_count = "select count(distinct address) as cnt from native_txs".to_string();
    let row = sqlx::query(sql_addr_count.as_str())
        .fetch_one(&mut conn)
        .await?;
    let active_addrs = row.try_get("cnt")?;

    // daily txs
    let start_time = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let sql_daily_txs = format!(
        "select count(*) as cnt from transaction where timestamp>={}",
        start_time.timestamp()
    );
    let row = sqlx::query(sql_daily_txs.as_str())
        .fetch_one(&mut conn)
        .await?;
    let daily_txs = row.try_get("cnt")?;

    Ok(V2ChainStatisticsResponse::Ok(Json(
        V2ChainStatisticsResult {
            code: 200,
            message: "".to_string(),
            data: Some(V2StatisticsData {
                active_addrs,
                total_txs,
                daily_txs,
            }),
        },
    )))
}

#[derive(ApiResponse)]
pub enum V2DistributeResponse {
    #[oai(status = 200)]
    Ok(Json<V2DistributeResult>),
    #[oai(status = 500)]
    InternalError(Json<V2DistributeResult>),
}

#[derive(Serialize, Deserialize, Default, Object)]
pub struct V2DistributeResult {
    pub code: i32,
    pub message: String,
    pub data: Option<V2TxsDistribute>,
}

#[derive(Serialize, Deserialize, Default, Object)]
pub struct V2TxsDistribute {
    pub transparent: i64,
    pub privacy: i64,
    pub prism: i64,
    pub evm_compatible: i64,
}

pub async fn v2_distribute(api: &Api) -> Result<V2DistributeResponse> {
    let mut conn = api.storage.lock().await.acquire().await?;

    let sql_native = "select count(*) as cnt from native_txs";
    let row_native = sqlx::query(sql_native).fetch_one(&mut conn).await?;
    let native_count: i64 = row_native.try_get("cnt")?;

    let sql_hide_amount_or_type =  "SELECT count(*) as cnt FROM native_txs WHERE (content @? '$.TransferAsset.body.transfer.outputs[*].asset_type.Confidential') OR (content @? '$.TransferAsset.body.transfer.outputs[*].amount.Confidential')";
    let row_hide_amount_or_type = sqlx::query(sql_hide_amount_or_type)
        .fetch_one(&mut conn)
        .await?;
    let hide_amount_or_type_count: i64 = row_hide_amount_or_type.try_get("cnt")?;

    let sql_hide_amount_and_type = "SELECT count(*) as cnt FROM native_txs WHERE (content @? '$.TransferAsset.body.transfer.outputs[*].asset_type.Confidential') AND (content @? '$.TransferAsset.body.transfer.outputs[*].amount.Confidential')";
    let row_sql_hide_amount_and_type = sqlx::query(sql_hide_amount_and_type)
        .fetch_one(&mut conn)
        .await?;
    let hide_amount_and_type_count: i64 = row_sql_hide_amount_and_type.try_get("cnt")?;

    let sql_evm = "SELECT count(*) as cnt FROM evm_txs";
    let row_evm = sqlx::query(sql_evm).fetch_one(&mut conn).await?;
    let evm_count: i64 = row_evm.try_get("cnt")?;

    let sql_prism_n2e = "select count(*) as cnt from n2e";
    let row_n2e = sqlx::query(sql_prism_n2e).fetch_one(&mut conn).await?;
    let n2e_count: i64 = row_n2e.try_get("cnt")?;

    let sql_prism_e2n = "select count(*) as cnt from e2n";
    let row_e2n = sqlx::query(sql_prism_e2n).fetch_one(&mut conn).await?;
    let e2n_count: i64 = row_e2n.try_get("cnt")?;

    let privacy = hide_amount_or_type_count + hide_amount_and_type_count;
    let transparent = native_count - privacy;
    let prism = n2e_count + e2n_count;
    let evm_compatible = evm_count;

    Ok(V2DistributeResponse::Ok(Json(V2DistributeResult {
        code: 200,
        message: "".to_string(),
        data: Some(V2TxsDistribute {
            transparent,
            privacy,
            prism,
            evm_compatible,
        }),
    })))
}

pub async fn v2_address_count(
    api: &Api,
    start_time: Query<Option<i64>>,
    end_time: Query<Option<i64>>,
) -> Result<AddressCountResponse> {
    let mut conn = api.storage.lock().await.acquire().await?;
    let mut params: Vec<String> = vec![];
    if let Some(start_time) = start_time.0 {
        params.push(format!("timestamp > {start_time} "));
    }
    if let Some(end_time) = end_time.0 {
        params.push(format!("timestamp < {end_time} "));
    }

    let mut sql_native = "select count(distinct address) as cnt from native_txs ".to_string();
    let mut sql_evm = "select count(distinct sender) as cnt from evm_txs ".to_string();

    if !params.is_empty() {
        sql_native = sql_native.add("WHERE ").add(params.join(" AND ").as_str());
        sql_evm = sql_evm.add("WHERE ").add(params.join(" AND ").as_str());
    }

    let row_native = sqlx::query(sql_native.as_str())
        .fetch_one(&mut conn)
        .await?;
    let native_count: i64 = row_native.try_get("cnt")?;

    let row_evm = sqlx::query(sql_evm.as_str()).fetch_one(&mut conn).await?;
    let evm_count: i64 = row_evm.try_get("cnt")?;

    Ok(AddressCountResponse::Ok(Json(AddressCountResult {
        code: 200,
        message: "".to_string(),
        data: Some(AddressCount {
            address_count: native_count + evm_count,
        }),
    })))
}