use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RefreshToken::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RefreshToken::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RefreshToken::AccountId).uuid().not_null())
                    .col(
                        ColumnDef::new(RefreshToken::TokenHash)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RefreshToken::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RefreshToken::RevokedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(RefreshToken::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_refresh_token_account")
                            .from(RefreshToken::Table, RefreshToken::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_refresh_token_account_id")
                    .table(RefreshToken::Table)
                    .col(RefreshToken::AccountId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RefreshToken::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RefreshToken {
    Table,
    Id,
    AccountId,
    TokenHash,
    ExpiresAt,
    RevokedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
}
