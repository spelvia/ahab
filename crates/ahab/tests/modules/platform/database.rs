//! The database keeps times as integers, so they compare to the microsecond
//! on SQLite and Postgres alike, whatever digits their fractions need.

use time::macros::datetime;

use ahab::modules::machines::machine::model::{Machine, MachineName, PublicKey};
use ahab::modules::machines::machine::store::{DbMachineRepository, MachineRepository};

use crate::support::TestDatabase;

#[tokio::test]
async fn times_compare_to_the_microsecond() {
    let database = TestDatabase::new().await;
    let machines = DbMachineRepository::new(database.db.clone());
    // As RFC 3339 text, `.35948Z` sorts after `.359481Z`.
    let earlier = datetime!(2026-11-17 09:00:00.35948 UTC);
    let later = datetime!(2026-11-17 09:00:00.359481 UTC);
    for (name, key, at) in [("earlier", 1, earlier), ("later", 2, later)] {
        let machine = Machine::pending(
            MachineName::parse(name).unwrap(),
            PublicKey::from_bytes(&[key; 32]).unwrap(),
            at,
        );
        machines.insert(&machine).await.unwrap();
    }

    assert_eq!(machines.delete_pending_before(later).await.unwrap(), 1);
    let left = machines.list().await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name.as_str(), "later");
    assert_eq!(left[0].created_at, later, "kept to the microsecond");
}
