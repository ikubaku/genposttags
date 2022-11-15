use std::collections::HashMap;
use std::time::Duration;
use anyhow::bail;
use chrono::NaiveDateTime;

use clap::Parser;

use flexi_logger::{FileSpec, Logger, WriteMode};

use log::{error, info, warn};

use futures::TryStreamExt;

use indicatif::{ProgressBar, ProgressStyle};

use regex::Regex;

use tokio::io::{AsyncReadExt};
use tokio::fs::File;

use sea_orm::{ColumnTrait, ConnectionTrait, CursorTrait, Database, EntityTrait, PaginatorTrait, QueryFilter, Schema, Statement};
use sea_orm::ActiveValue::Set;
use sea_query::Condition;

mod config;
mod entities;

use entities::prelude::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config_path: String,

    #[arg(short, long)]
    log_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    if let Some(log_path) = args.log_path {
        let _logger = Logger::try_with_str("info")?
            .log_to_file(FileSpec::try_from(log_path)?)
            .write_mode(WriteMode::BufferAndFlush)
            .start()?;
    } else {
        let _logger = Logger::try_with_str("info")?
            .log_to_stderr()
            .start()?;
    }
    let mut conf_file = File::open(&args.config_path).await?;
    let mut conf_str = String::new();
    conf_file.read_to_string(&mut conf_str).await?;
    let config: config::Config = toml::from_str(&conf_str)?;
    let db = Database::connect(config.database_url).await?;

    // Create the table.
    info!("Creating the destination table...");
    if config.allow_drop_destination_table {
        db.execute(Statement::from_string(
            db.get_database_backend(),
            format!("DROP TABLE IF EXISTS {};", config.destination_table_name),
        ))
            .await?;
    } else {
        let res = db.query_all(Statement::from_string(
                db.get_database_backend(),
                format!("SHOW TABLES LIKE \"{}\";", config.destination_table_name),
            ))
            .await?;
        if res.len() != 0 {
            error!("The destination table already exists. Refusing to work.");
            bail!("The destination table already exists. Refusing to work.");
        }
    };
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(PostTags)))
        .await?;
    info!("Created the destination table.");

    // Fetch the TagName-TagId relation
    let mut tag_info = HashMap::<String, i32>::new();
    let mut stream = Tags::find().stream(&db).await?;
    while let Some(tags_entry) = stream.try_next().await? {
        let Some(tag_name) = tags_entry.tag_name else { warn!("TagName should not be null for TagId: {}", tags_entry.id); continue; };
        tag_info.insert(tag_name, tags_entry.id);
    }

    // Collect histories
    info!("Scanning the PostHistory entries...");
    let condition = PostHistory::find().filter(Condition::any()
        .add(entities::post_history::Column::PostHistoryTypeId.eq(3))
        .add(entities::post_history::Column::PostHistoryTypeId.eq(6))
        .add(entities::post_history::Column::PostHistoryTypeId.eq(9)));
    let count = condition.clone().count(&db).await?;
    info!("Total entry count: {}", count);
    info!("Showing first 10 PostHistory entries...");
    let res = condition.clone()
        .cursor_by(entities::post_history::Column::Id)
        .first(10)
        .all(&db).await?;
    for hist in res {
        info!("{:?}", hist);
    }
    info!("Collecting data...");
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(Duration::from_millis(200));
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.dim.bold} Processing ID: {wide_msg}")?
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    let mut last_update = HashMap::<i32, NaiveDateTime>::new();
    let mut post_tags_by_name_list = HashMap::<i32, String>::new();
    let mut stream = condition.stream(&db).await?;
    while let Some(hist) = stream.try_next().await? {
        spinner.set_message(format!("{}", hist.id));
        let type_id = hist.post_history_type_id;
        let post_id = hist.post_id;
        let Some(creation_date) = hist.creation_date else { warn!("CreationDate should not be null for ID: {}",  hist.id); continue; };
        let Some(text) = hist.text else { warn!("Text should not be null for ID: {}", hist.id); continue; };
        if let Some(this_last_update) = last_update.get(&post_id) {
            if *this_last_update < creation_date {
                if type_id == 3 {
                    warn!("More than one Initial Tags history event exist for PostId: {}. Overwriting previous records.", post_id);
                }
                last_update.insert(post_id, creation_date);
                post_tags_by_name_list.insert(post_id, text);
            }
        } else {
            if type_id != 3 {
                warn!("No Initial Tags history event present before this ID: {} for PostId: {}", hist.id, post_id);
            }
            last_update.insert(post_id, creation_date);
            post_tags_by_name_list.insert(post_id, text);
        }
    }
    spinner.finish();
    info!("Inserting collected data...");
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(Duration::from_millis(200));
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.dim.bold} Inserting for PostId: {wide_msg}")?
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    let pattern = Regex::new(r"<([^<>]*?)>")?;
    for (post_id, tag_name_text) in post_tags_by_name_list {
        spinner.set_message(format!("{}", post_id));
        for match_res in pattern.captures_iter(&tag_name_text) {
            let Some(tag_id) = tag_info.get(&match_res[1]) else {warn!("No tag found for TagName: {}", &match_res[1]); continue; };
            let post_tags_entry = entities::post_tags::ActiveModel {
                post_id: Set(post_id),
                tag_id: Set(*tag_id),
                ..Default::default()
            };
            PostTags::insert(post_tags_entry).exec(&db).await?;
        }
    }
    spinner.finish();

    info!("Done.");

    Ok(())
}
