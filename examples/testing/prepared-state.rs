use crate::{
    content::{Content, WIZARD},
    store::{InventoryWrite, Store},
};
use anyhow::{Context, Result, ensure};
use rand::{SeedableRng, rngs::StdRng};
use std::path::Path;

#[tokio::test]
#[ignore = "creates disposable QA saves; set ROTMG_QA_GUID to an owned QA browser's guest GUID"]
async fn example_prepared_state() -> Result<()> {
    let guid =
        std::env::var("ROTMG_QA_GUID").context("set ROTMG_QA_GUID for a disposable QA profile")?;
    let state = std::env::temp_dir().join(format!("openrotmg-qa-{:016x}", rand::random::<u64>()));
    // Exclusive creation prevents this recipe from ever opening an existing save directory.
    let mut directory = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(&state)?;
    ensure!(
        std::fs::create_dir(&state).is_err(),
        "QA directory was not exclusive"
    );
    let content = Content::load(Path::new("data"))?;
    let store = Store::open(&state.join("game.db"), &content)?;
    let account = store.list(guid.clone()).await?;
    let mut character = store.create(account.id, content.get(WIZARD)?).await?;
    character.gain_experience(&content, 2450, &mut StdRng::seed_from_u64(12351))?;
    character.items[0] = 2712;
    character.items[4] = 2594;
    let stats = character.effective(&content)?;
    character.hp = stats[0];
    character.mp = stats[1];
    character.revision = store
        .exchange(
            vec![InventoryWrite {
                id: character.id,
                account: account.id,
                revision: character.revision,
                items: character.items.clone(),
            }],
            None,
        )
        .await?[0];
    store.progress(character.clone()).await?;
    let saved = store.load(account.id, character.id).await?;
    ensure!(
        saved.level == 8 && saved.items[0] == 2712,
        "prepared character did not persist"
    );
    std::fs::write(
        state.join("qa-example.json"),
        serde_json::to_vec(&serde_json::json!({
            "purpose": "prepared QA state, NOT earned progression evidence",
            "character": saved.id, "level": saved.level
        }))?,
    )?;
    println!(
        "Prepared QA only: state={} character={} level={}",
        state.display(),
        saved.id,
        saved.level
    );
    Ok(())
}
