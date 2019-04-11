use failure::Fail;
use uuid::Uuid;

use crate::domain::{Aggregate, Generation};

#[derive(Debug, Eq, PartialEq)]
pub struct Toggle {
    // Universally unique identifier
    id: Uuid,
    // For optimistic locking
    generation: Generation,
    // Human readable name
    name: String,
    // For evolving Toggles
    version: i32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    Created { id: Uuid, name: String },
}

#[derive(Debug, Fail)]
pub enum ToggleError {
    #[fail(display = "invalid name: {}", name)]
    InvalidName { name: String },
    #[fail(display = "invalid event `{}` applied to state `{}", event, state)]
    InvalidStateEvent { state: String, event: String },
}

impl Toggle {
    pub fn create(id: Uuid, name: String) -> Result<Vec<Event>, ToggleError> {
        Ok(vec![Event::Created { id: id, name: name }])
    }
}

impl Aggregate for Toggle {
    type Id = Uuid;
    type Event = Event;
    type Err = ToggleError;

    fn id(&self) -> &Self::Id {
        &self.id
    }

    fn generation(&self) -> Generation {
        self.generation
    }

    fn apply_event(state: Option<Self>, event: &Self::Event) -> Result<Self, Self::Err> {
        match (&state, event) {
            (None, Event::Created { id, name }) => Ok(Toggle {
                id: *id,
                generation: Generation::first(),
                name: name.clone(),
                version: 0,
            }),
            _ => Err(ToggleError::InvalidStateEvent {
                state: format!("{:?}", state),
                event: format!("{:?}", event),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use failure::Error;
    use uuid::Uuid;

    use super::{Event, Toggle};

    #[test]
    fn test_create() -> Result<(), Error> {
        let id = Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8")?;
        let events = Toggle::create(id, "test".to_owned())?;
        assert_eq!(
            events,
            vec![Event::Created {
                id: id,
                name: "test".to_owned()
            },],
        );
        Ok(())
    }
}
