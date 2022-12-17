use crate::prelude::*;

#[derive(Clone, Bundle)]
pub struct DynamicColliderBundle {
    pub collider: Collider,
    pub collider_scale: ColliderScale,
    pub rigid_body: RigidBody,
    pub velocity: Velocity,
    pub locked_axes: LockedAxes,
    pub restitution: Restitution,
    pub friction: Friction,
    pub active_events: ActiveEvents,
    pub ccd: Ccd,
    pub collision_groups: CollisionGroups,
}

impl Default for DynamicColliderBundle {
    fn default() -> Self {
        Self {
            collider: Collider::cuboid(1., 1.),
            collider_scale: ColliderScale::Absolute(Vec2::new(1., 1.)),
            rigid_body: RigidBody::Dynamic,
            velocity: Velocity::zero(),
            locked_axes: LockedAxes::default(),
            restitution: Restitution::default(),
            friction: Friction::default(),
            active_events: ActiveEvents::empty(),
            ccd: Ccd::disabled(),
            collision_groups: CollisionGroups::default(),
        }
    }
}

#[derive(Clone, Bundle)]
pub struct FixedColliderBundle {
    pub collider: Collider,
    pub collider_scale: ColliderScale,
    pub rigid_body: RigidBody,
    pub locked_axes: LockedAxes,
    pub restitution: Restitution,
    pub friction: Friction,
    pub collision_groups: CollisionGroups,
}

impl Default for FixedColliderBundle {
    fn default() -> Self {
        Self {
            collider: Collider::cuboid(1., 1.),
            collider_scale: ColliderScale::Absolute(Vec2::from((1., 1.))),
            rigid_body: RigidBody::Fixed,
            locked_axes: LockedAxes::all(),
            restitution: Restitution::default(),
            friction: Friction::default(),
            collision_groups: CollisionGroups::default(),
        }
    }
}
