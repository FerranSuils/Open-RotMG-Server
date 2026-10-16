use super::*;

impl World {
    pub fn ground_damage(&mut self, id: i32, time: i32, pos: Pos) -> Result<()> {
        ensure!(
            time >= 0 && pos.x.is_finite() && pos.y.is_finite(),
            "invalid ground damage report"
        );
        let p = self
            .players
            .get(&id)
            .context("ground damage player missing")?;
        ensure!(
            p.pos.distance(pos) <= 3.,
            "ground damage position is outside the native movement window"
        );
        let c = self
            .map
            .cell(pos.x, pos.y)
            .context("ground damage outside map")?;
        let ground = c.ground;
        let hazard = self
            .content
            .hazards
            .get(&ground)
            .context("ground report has no hazard")?
            .clone();
        let i = pos.y as usize * self.map.width as usize + pos.x as usize;
        let protected = self.cells[i]
            .iter()
            .any(|id| self.content.objects[&self.entities[id].kind].protects_ground);
        let p = self.players.get_mut(&id).unwrap();
        let ignored = self.safe
            || protected
            || !p.character.alive
            || p.paused
            || p.departing
            || p.death_pending
            || p.ground_hits
                .get(&i)
                .is_some_and(|previous| time.saturating_sub(*previous) < 500);
        if ignored && p.controller != Controller::Human {
            return Ok(());
        }
        // A native report means its client already consumed this draw, even if a later state protects it.
        let damage = p.roll_between(hazard.min, hazard.max);
        if ignored {
            return Ok(());
        }
        p.ground_hits
            .retain(|_, previous| time.saturating_sub(*previous) < 1000);
        p.ground_hits.insert(i, time);
        let damage = crate::condition::damage(damage, 0, p.conditions(self.now), true);
        p.character.hp = (p.character.hp - damage).max(0);
        if p.character.hp == 0 {
            p.death_pending = true;
            p.killer = hazard.name;
            p.bot_goal = None;
            p.bot_aim = None;
        }
        tracing::debug!(
            player = id,
            ground,
            damage,
            hp = p.character.hp,
            "ground damage committed"
        );
        Ok(())
    }
    pub(super) fn ground_step(&mut self) {
        let actors: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| {
                p.controller != Controller::Human
                    && p.character.alive
                    && !p.paused
                    && !p.departing
                    && !p.death_pending
            })
            .filter(|(_, p)| {
                self.map
                    .cell(p.pos.x, p.pos.y)
                    .is_some_and(|c| self.content.hazards.contains_key(&c.ground))
            })
            .map(|(id, p)| (*id, p.pos))
            .collect();
        for (id, pos) in actors {
            if let Err(error) = self.ground_damage(id, self.now as i32, pos) {
                tracing::error!(player=id,%error,"actor ground damage refused");
            }
        }
    }
}
