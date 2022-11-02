use parking_lot::Mutex;
use crate::{
    builder::WrappedClient,
    twilight_exports::*,
    waiter::{InteractionWaiter, WaiterWaker}
};

use crate::iter::DataIterator;
use crate::parse::{Parse, ParseError};
use crate::waiter::new_pair;

/// The value the user is providing to the argument.
#[derive(Debug, Clone)]
pub struct Focused {
    pub input: String,
    pub kind: CommandOptionType,
}

/// Context given to all functions used to autocomplete arguments.
pub struct AutocompleteContext<'a, D> {
    /// The http client used by the framework.
    pub http_client: &'a WrappedClient,
    /// The data shared across the framework.
    pub data: &'a D,
    /// The user input.
    pub user_input: Focused,
    /// The interaction itself.
    pub interaction: &'a mut Interaction,
}

impl<'a, D> AutocompleteContext<'a, D> {
    pub(crate) fn new(
        http_client: &'a WrappedClient,
        data: &'a D,
        user_input: Focused,
        interaction: &'a mut Interaction,
    ) -> Self {
        Self {
            http_client,
            data,
            user_input,
            interaction,
        }
    }

    /// Gets the http client used by the framework.
    pub fn http_client(&self) -> &Client {
        self.http_client.inner()
    }
}

/// Framework context given to all command functions, this struct contains all the necessary
/// items to respond the interaction and access shared data.
pub struct SlashContext<'a, D> {
    /// The http client used by the framework.
    pub http_client: &'a WrappedClient,
    /// The application id provided to the framework.
    pub application_id: Id<ApplicationMarker>,
    /// An [interaction client](InteractionClient) made out of the framework's [http client](Client)
    pub interaction_client: InteractionClient<'a>,
    /// The data shared across the framework.
    pub data: &'a D,
    /// Components waiting for an interaction.
    pub waiters: &'a Mutex<Vec<WaiterWaker>>,
    /// The interaction itself.
    pub interaction: Interaction,
}

impl<'a, D> Clone for SlashContext<'a, D> {
    fn clone(&self) -> Self {
        SlashContext {
            http_client: self.http_client,
            application_id: self.application_id,
            interaction_client: self.http_client.inner().interaction(self.application_id),
            data: self.data,
            waiters: self.waiters,
            interaction: self.interaction.clone(),
        }
    }
}

impl<'a, D> SlashContext<'a, D> {
    /// Creates a new context.
    pub(crate) fn new(
        http_client: &'a WrappedClient,
        application_id: Id<ApplicationMarker>,
        data: &'a D,
        waiters: &'a Mutex<Vec<WaiterWaker>>,
        interaction: Interaction,
    ) -> Self {
        let interaction_client = http_client.inner().interaction(application_id);
        Self {
            http_client,
            application_id,
            interaction_client,
            data,
            waiters,
            interaction,
        }
    }

    /// Gets the http client used by the framework.
    pub fn http_client(&self) -> &Client {
        self.http_client.inner()
    }

    /// Responds to the interaction with an empty message to allow to respond later.
    ///
    /// When this method is used [update_response](Self::update_response) has to be used to edit the response.
    pub async fn acknowledge<E>(&self) -> Result<(), E>
    where
        E: From<twilight_http::Error>
    {
        self.interaction_client
            .create_response(
                self.interaction.id,
                &self.interaction.token,
                &InteractionResponse {
                    kind: InteractionResponseType::DeferredChannelMessageWithSource,
                    data: None,
                },
            )
            .exec()
            .await
            .map_err(From::from)?;

        Ok(())
    }

    /// Updates the sent interaction, this method is a shortcut to twilight's
    /// [update_interaction_original](InteractionClient::update_response)
    /// but http is automatically provided.
    pub async fn update_response<F>(
        &'a self,
        fun: F,
    ) -> Result<Message, twilight_http::Error>
    where
        F: FnOnce(UpdateResponse<'a>) -> UpdateResponse<'a>,
    {
        let update = fun(self
            .interaction_client
            .update_response(&self.interaction.token));
        Ok(update
            .exec()
            .await?
            .model()
            .await?)
    }

    pub fn wait_interaction<F>(&self, fun: F) -> InteractionWaiter
    where
        F: Fn(&Interaction) -> bool + Send + 'static
    {
        let (waker, waiter) = new_pair(fun);
        let mut lock = self.waiters.lock();
        lock.push(waker);
        waiter
    }
}

impl<D: Send + Sync> SlashContext<'_, D> {
    #[doc(hidden)]
    pub async fn named_parse<T>(
        &self,
        name: &str,
        iterator: &mut DataIterator<'_>
    ) -> Result<T, ParseError>
    where
        T: Parse<D>
    {
        let value = iterator.get(|s| s.name == name);
        if value.is_none() && <T as Parse<D>>::required() {
            Err(ParseError::StructureMismatch(format!("{} not found", name)).into())
        } else {
            <T as Parse<D>>::parse(self.http_client, self.data, value.map(|it| &it.value)).await
                .map_err(|mut err| {
                    if let ParseError::Parsing { argument_name, .. } = &mut err {
                        *argument_name = name.to_string();
                    }
                    err.into()
                })
        }
    }
}
