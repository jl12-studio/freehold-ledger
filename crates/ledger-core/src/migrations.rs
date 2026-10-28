/// Numbered migrations, applied in order. Append only; never edit a shipped entry.
pub(crate) const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/0001_init.sql"))];
