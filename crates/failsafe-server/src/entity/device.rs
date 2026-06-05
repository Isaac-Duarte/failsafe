use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "device")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub device_id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub iroh_public_key: String,
    pub enabled_features: Json,
    pub last_seen: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::account::Entity",
        from = "Column::AccountId",
        to = "super::account::Column::Id"
    )]
    Account,
}

impl Related<super::account::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Account.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
