pub mod analysis;
pub mod data_apis;
pub mod graphql;
pub mod math;
pub mod utils;

use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use std::convert::Infallible;
use warp::{
    http::{Response, StatusCode},
    Filter, Rejection,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let graphql_filter = async_graphql_warp::graphql(graphql::schema()).and_then(
        |(schema, request): (graphql::Schema, async_graphql::Request)| async move {
            let resp = schema.execute(request).await;
            Ok::<_, Infallible>(async_graphql_warp::Response::from(resp))
        },
    );

    let graphql_playground = warp::path::end().and(warp::get()).map(|| {
        Response::builder()
            .header("content-type", "text/html")
            .body(playground_source(GraphQLPlaygroundConfig::new("/")))
    });

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "PUT", "OPTIONS"])
        .allow_header("content-type");

    let routes = graphql_playground.or(graphql_filter);

    warp::serve(routes.recover(handle_rejection).with(cors))
        .run(([127, 0, 0, 1], 3030))
        .await;

    Ok(())
}

async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::with_status(
        format!("{:?}", err),
        StatusCode::INTERNAL_SERVER_ERROR,
    ))
}
