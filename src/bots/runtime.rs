use super::*;
use crate::{
    server::{self, App},
    store::Character,
    world::Output,
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

async fn enter(
    app: &App,
    profile: Profile,
    character: Character,
    world: i32,
) -> Result<(i32, mpsc::Receiver<Packet>, Brain)> {
    let world = if world == -3 {
        app.world.directory(|d| d.initial(-3)).await?
    } else {
        world
    };
    // Concurrent novas and hit receipts must fit between bounded perception updates.
    let (packets, output) = mpsc::channel(OUTPUT_CAPACITY);
    let (lost, _) = watch::channel(false);
    let id = app
        .world
        .in_world(world)
        .call(move |w| w.enter_autonomous(character, Output { packets, lost }))
        .await?;
    let view = app.world.directory(move |d| d.player_view(id)).await?;
    tracing::info!(
        player = id,
        name = profile.name,
        class = view.character.class,
        level = view.character.level,
        character = view.session.character,
        account = view.session.account,
        experience = view.character.experience,
        hp = view.character.hp,
        mp = view.character.mp,
        items = ?view.character.items,
        revision = view.character.inventory_revision,
        "autonomous character entered"
    );
    Ok((id, output, Brain::new(profile, &view)))
}

pub async fn start(app: &App) -> Result<()> {
    for profile in PROFILES {
        let character = app
            .store
            .shore_character(profile.name.into(), &app.content, profile.healer)
            .await?;
        let account = character.account;
        let owner = app.claim(account)?;
        let (mut id, mut output, mut brain) = enter(app, profile, character, -3).await?;
        let app = app.clone();
        tokio::spawn(async move {
            let mut shutdown = app.shutdown.subscribe();
            let result: Result<()> = async {
                loop {
                    tokio::select! {
                        _=tokio::time::sleep(Duration::from_millis(50))=>{},
                        _=shutdown.changed()=>break,
                    }
                    let view = app.world.directory(move |d| d.player_view(id)).await?;
                    ensure!(view.session.account == account, "autonomous account binding changed");
                    if view.lifecycle.death_pending {
                        while output.try_recv().is_ok() {}
                        continue;
                    }
                    if !view.lifecycle.alive {
                        server::depart(&app, id).await?;
                        tokio::select! {
                            _=tokio::time::sleep(Duration::from_secs(3))=>{},
                            _=shutdown.changed()=>return Ok(()),
                        }
                        let character = app.store.shore_character(profile.name.into(), &app.content, profile.healer).await?;
                        ensure!(character.account == account && character.id != view.session.character,
                            "replacement did not preserve account and replace the dead character");
                        (id, output, brain) = enter(&app, profile, character, -3).await?;
                        continue;
                    }
                    ensure!(matches!(view.lifecycle.output, OutputState::Live)
                        && !view.lifecycle.departing && !view.lifecycle.quarantined, "controller is not live");
                    while let Ok(packet) = output.try_recv() {
                        if brain.receive(packet, &view)? {
                            if let Err(error) = app.world.directory(move |d| d.chat(id, "omw".into(), Instant::now())).await {
                                tracing::warn!(player=id,%error,"invitation acknowledgement rejected");
                            }
                            tracing::info!(player=id, companion=brain.companion, "visible invitation accepted");
                        }
                    }
                    if view.lifecycle.busy || view.tick < brain.next_decision { continue; }
                    let decision = brain.decide(&view, &app.content)?;
                    if let Some(text) = decision.chat {
                        match app.world.directory(move |d| d.chat(id, text.into(), Instant::now())).await {
                            Ok(()) => tracing::info!(player=id,text,"autonomous public call delivered"),
                            Err(error) => tracing::warn!(player=id,text,%error,"autonomous public call rejected"),
                        }
                    }
                    if decision.goal != brain.goal {
                        tracing::info!(player=id,name=profile.name,tick=view.tick,hp=view.character.hp,
                            experience=view.character.experience,x=view.position.x,y=view.position.y,
                            goal=?decision.goal,"autonomous decision");
                    }
                    brain.goal = decision.goal;
                    brain.destination = decision.movement;
                    let mut actions = vec![Action::Stop];
                    if decision.journey.is_none() {
                        if let Some(pos) = decision.movement { actions.push(Action::Move { x:pos.x, y:pos.y }); }
                        if let Some(angle) = decision.aim { actions.push(Action::Shoot { angle }); }
                        if let Some(action) = decision.item { actions.push(action); }
                    }
                    let diagnostic = format!("{actions:?}");
                    match app.world.for_player(id).call(move |w| {
                        let mut plan = None;
                        for action in actions {
                            if let Some(item) = w.autonomous_action(id, action)? { plan = Some(item); }
                        }
                        Ok(plan)
                    }).await {
                        Ok(Some(plan)) => server::persist(&app, plan).await?,
                        Ok(None) => {},
                        Err(error) => {
                            if let Some(pos) = brain.destination { brain.blocked.insert(tile(pos), view.tick + 100); }
                            tracing::warn!(player=id,%error,actions=diagnostic,"autonomous commands rejected; route excluded and will reassess");
                            continue;
                        }
                    }
                    if let Some(journey) = decision.journey {
                        match journey {
                            Journey::Regroup(target) => {
                                if let Err(error) = app.world.directory(move |d| d.teleport(id, target, Instant::now())).await {
                                    tracing::warn!(player=id,target,%error,"autonomous regroup rejected");
                                }
                            }
                            Journey::Portal(_) | Journey::Nexus => {
                                let portal = match journey { Journey::Portal(portal) => Some(portal), Journey::Nexus => None, Journey::Regroup(_) => unreachable!() };
                                let request = app.world.directory(move |d| {
                                    let (account, character, destination) = d.destination(id, portal)?;
                                    let (time, key, _) = d.issue(account, character, destination)?;
                                    Ok((destination, time, key))
                                }).await;
                                let (destination, time, key) = match request {
                                    Ok(request) => request,
                                    Err(error) => {
                                        brain.portal = None;
                                        tracing::warn!(player=id,?portal,%error,"autonomous transfer rejected before departure");
                                        continue;
                                    }
                                };
                                server::depart(&app, id).await?;
                                let character_id = app.world.directory(move |d| d.consume(account, destination, time, key)).await?;
                                ensure!(character_id == view.session.character, "transfer character binding changed");
                                let character = app.store.load(account, character_id).await?;
                                (id, output, brain) = enter(&app, profile, character, destination).await?;
                                tracing::info!(player=id,account,character=character_id,destination,?portal,"autonomous durable transfer completed");
                            }
                        }
                    }
                }
                server::depart(&app, id).await
            }.await;
            if let Err(error) = result {
                tracing::error!(player=id,name=profile.name,%error,"autonomous controller stopped; account retained for diagnosis");
                let reason = error.to_string();
                if let Err(error) = app
                    .world
                    .for_player(id)
                    .call(move |w| w.poison(id, reason))
                    .await
                {
                    tracing::error!(player=id,%error,"autonomous quarantine failed");
                }
                app.quarantined
                    .lock()
                    .expect("quarantine registry poisoned")
                    .push(owner);
            }
        });
    }
    Ok(())
}
