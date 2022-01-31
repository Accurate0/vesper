use crate::{
    command::CommandResult,
    message::Message,
    twilight_exports::*,
    waiter::{WaiterReceiver, WaiterSender},
};
use parking_lot::Mutex;

/// Framework context given to all command functions, this struct contains all the necessary
/// items to respond the interaction and access shared data.
pub struct SlashContext<'a, D> {
    /// The [http client](Client) used by the framework.
    pub http_client: &'a Client,
    /// The application id provided to the framework.
    pub application_id: Id<ApplicationMarker>,
    /// An [interaction client](InteractionClient) made out of the framework's [http client](Client)
    pub interaction_client: InteractionClient<'a>,
    /// The data shared across the framework.
    pub data: &'a D,
    waiters: &'a Mutex<Vec<WaiterSender>>,
    /// The interaction itself.
    pub interaction: ApplicationCommand,
}

impl<'a, D> Clone for SlashContext<'a, D> {
    fn clone(&self) -> Self {
        SlashContext {
            http_client: &self.http_client,
            application_id: self.application_id,
            interaction_client: self.http_client.interaction(self.application_id),
            data: &self.data,
            waiters: &self.waiters,
            interaction: self.interaction.clone(),
        }
    }
}

impl<'a, D> SlashContext<'a, D> {
    /// Creates a new context.
    pub(crate) fn new(
        http_client: &'a Client,
        application_id: Id<ApplicationMarker>,
        data: &'a D,
        waiters: &'a Mutex<Vec<WaiterSender>>,
        interaction: ApplicationCommand,
    ) -> Self {
        let interaction_client = http_client.interaction(application_id);
        Self {
            http_client,
            application_id,
            interaction_client,
            data,
            waiters,
            interaction,
        }
    }

    /// Responds to the interaction with an empty message to allow to respond later.
    ///
    /// When this method is used [update_response](Self::update_response) has to be used to edit the response.
    pub async fn acknowledge(&self) -> CommandResult {
        self.interaction_client
            .interaction_callback(
                self.interaction.id,
                &self.interaction.token,
                &InteractionResponse::DeferredChannelMessageWithSource(CallbackData {
                    allowed_mentions: None,
                    components: None,
                    content: None,
                    embeds: None,
                    flags: None,
                    tts: None,
                }),
            )
            .exec()
            .await?;

        Ok(())
    }

    /// Updates the sent interaction, this method is a shortcut to twilight's
    /// [update_interaction_original](InteractionClient::update_interaction_original)
    /// but http is automatically provided.
    pub async fn update_response<F>(
        &'a self,
        fun: F,
    ) -> Result<Message<'a, D>, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce(UpdateOriginalResponse<'a>) -> UpdateOriginalResponse<'a>,
    {
        let update = fun(self.interaction_client.update_interaction_original(&self.interaction.token));
        Ok(update
            .exec()
            .await?
            .model()
            .await
            .map(|msg| Message::new(&self, msg))?)
    }

    /// Waits for a component interaction which satisfies the given predicate.
    pub fn wait_component<F>(&self, fun: F) -> WaiterReceiver
    where
        F: Fn(&MessageComponentInteraction) -> bool + Send + 'static,
    {
        let (sender, receiver) = WaiterSender::new(fun);
        {
            let mut lock = self.waiters.lock();
            lock.push(sender);
        }
        receiver
    }
}
