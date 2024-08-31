use crate::prelude::*;

#[derive(Clone, Bundle)]
pub struct DynamicColliderBundle {
    pub collider: Collider,
    pub rigid_body: RigidBody,
    pub linear_velocity: LinearVelocity,
    pub angular_velocity: AngularVelocity,
    pub locked_axes: LockedAxes,
    pub restitution: Restitution,
    pub friction: Friction,
    //pub ccd: Ccd,
}

impl Default for DynamicColliderBundle {
    fn default() -> Self {
        Self {
            collider: Collider::rectangle(1., 1.),
            rigid_body: RigidBody::Dynamic,
            linear_velocity: LinearVelocity::ZERO,
            angular_velocity: AngularVelocity::ZERO,
            locked_axes: LockedAxes::default(),
            restitution: Restitution::default(),
            friction: Friction::default(),
            //ccd: Ccd::disabled(),
        }
    }
}

#[derive(Clone, Bundle)]
pub struct FixedColliderBundle {
    pub collider: Collider,
    pub rigid_body: RigidBody,
    pub locked_axes: LockedAxes,
    pub restitution: Restitution,
    pub friction: Friction,
}

impl Default for FixedColliderBundle {
    fn default() -> Self {
        Self {
            collider: Collider::rectangle(1., 1.),
            rigid_body: RigidBody::Static,
            locked_axes: LockedAxes::ALL_LOCKED,
            restitution: Restitution::default(),
            friction: Friction::default(),
        }
    }
}
