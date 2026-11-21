use super::ConditionWatcher;
use anyhow::{Context, Result};
use async_io::Timer;
use futures_lite::{Stream, StreamExt, future, stream};
use std::pin::Pin;
use std::time::Duration;
use zbus::{
    Connection, MatchRule, MessageStream, Proxy, message::Message, zvariant::OwnedObjectPath,
};

const SECRETS: &str = "org.freedesktop.secrets";
const KEEPASSXC: &str = "org.keepassxc.KeePassXC.MainWindow";
const ROOT: &str = "/org/freedesktop/secrets";
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";

fn connection() -> Result<Connection> {
    future::block_on(
        zbus::connection::Builder::session()?
            .method_timeout(Duration::from_secs(5))
            .build(),
    )
    .context("Connect to the session D-Bus")
}

async fn unlocked(connection: &Connection) -> zbus::Result<bool> {
    let dbus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    let owner: String = dbus.call("GetNameOwner", &SECRETS).await?;
    let keepassxc: String = dbus.call("GetNameOwner", &KEEPASSXC).await?;
    if owner != keepassxc {
        return Ok(false);
    }
    // Address the verified owner directly: querying metadata must not activate another provider.
    let service = Proxy::new(connection, owner.as_str(), ROOT, SERVICE).await?;
    let path: OwnedObjectPath = service.call("ReadAlias", &"default").await?;
    if path.as_str() == "/" {
        return Ok(false);
    }
    let properties = Proxy::new(
        connection,
        owner.as_str(),
        path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await?;
    let locked: zbus::zvariant::OwnedValue =
        properties.call("Get", &(COLLECTION, "Locked")).await?;
    Ok(!bool::try_from(locked)?)
}

fn status(connection: &Connection) -> Result<bool> {
    match future::block_on(unlocked(connection)) {
        Err(zbus::Error::MethodError(name, _, _))
            if matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.UnknownObject"
            ) =>
        {
            Ok(false)
        }
        result => result.context("Read KeePassXC's default database lock state"),
    }
}

pub(super) fn is_unlocked() -> Result<bool> {
    status(&connection()?)
}

pub(super) struct Watcher {
    events: Pin<Box<dyn Stream<Item = zbus::Result<Message>> + Send>>,
}

impl Watcher {
    pub(super) fn new() -> Result<Self> {
        let connection = connection()?;
        let rules = [
            format!("type='signal',sender='{SECRETS}',interface='{SERVICE}',path='{ROOT}'"),
            format!(
                "type='signal',sender='{SECRETS}',interface='org.freedesktop.DBus.Properties',\
                 member='PropertiesChanged',path_namespace='{ROOT}/collection',arg0='{COLLECTION}'"
            ),
            format!(
                "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',\
                 member='NameOwnerChanged',arg0='{SECRETS}'"
            ),
            format!(
                "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',\
                 member='NameOwnerChanged',arg0='{KEEPASSXC}'"
            ),
        ];
        let events = future::block_on(async {
            let mut events: Pin<Box<dyn Stream<Item = zbus::Result<Message>> + Send>> = Box::pin(
                MessageStream::for_match_rule(
                    MatchRule::try_from(rules[0].as_str())?,
                    &connection,
                    None,
                )
                .await?,
            );
            // Register every match before the daemon's first state check to avoid missed unlocks.
            for rule in &rules[1..] {
                let rule = MatchRule::try_from(rule.as_str())?;
                // Drain existing subscriptions while the next AddMatch reply is pending.
                let messages = future::or(
                    MessageStream::for_match_rule(rule, &connection, None),
                    async {
                        while let Some(message) = events.next().await {
                            message?;
                        }
                        Err(zbus::Error::Failure(
                            "Session D-Bus event stream closed".into(),
                        ))
                    },
                )
                .await?;
                events = Box::pin(stream::or(events, messages));
            }
            Ok::<_, zbus::Error>(events)
        })
        .context("Subscribe to KeePassXC lock and service changes")?;
        Ok(Self { events })
    }
}

impl ConditionWatcher for Watcher {
    fn keepassxc_unlocked(&self) -> Result<bool> {
        // A separate connection keeps queued signals from delaying method replies.
        is_unlocked()
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<()> {
        future::block_on(async {
            let change = async {
                self.events
                    .next()
                    .await
                    .context("Session D-Bus event stream closed")??;
                Ok(())
            };
            match timeout {
                Some(timeout) => {
                    future::or(change, async {
                        Timer::after(timeout).await;
                        Ok(())
                    })
                    .await
                }
                None => change.await,
            }
        })
    }
}
