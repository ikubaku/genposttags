//! `SeaORM` Entity. Generated by sea-orm-codegen 0.10.3

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "PostHistoryType")]
pub struct Model {
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i8,
    #[sea_orm(column_name = "Type")]
    pub r#type: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::post_history::Entity")]
    PostHistory,
}

impl Related<super::post_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PostHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
