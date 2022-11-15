use serde_derive::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub(crate) database_url: String,
    only_these_tags: Option<Vec<String>>,
    pub(crate) destination_table_name: String,
    pub(crate) allow_drop_destination_table: bool,
}
