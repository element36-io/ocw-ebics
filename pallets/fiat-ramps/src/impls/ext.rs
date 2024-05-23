//! Functions related to statement processing
use crate::{types::*, *};

impl<T: Config> Pallet<T> {
	/// Process a single transaction
	///
	/// ### Arguments
	///
	/// - `statement_owner`: Owner of the statement we are processing
	/// - `statement_iban`: IBAN number of the statement we are processing
	/// - `source`: Source/sender of the transaction
	/// - `dest`: Destination/receiver of the transaction
	/// - `transaction`: Transaction data
	/// - `reference`: Optional reference field (usually contains burn request id)
	pub(crate) fn process_transaction(
		statement_owner: &AccountIdOf<T>,
		statement_iban: &IbanOf<T>,
		source: Option<T::AccountId>,
		dest: Option<T::AccountId>,
		transaction: &TransactionOf<T>,
		reference: Option<u64>,
	) -> DispatchResult {
		let amount: BalanceOf<T> = BalanceOf::<T>::try_from(transaction.amount).unwrap_or_default();

		// Process transaction based on its type
		match transaction.tx_type {
			TransactionType::Incoming => {
				log::debug!("[OCW] Incoming transaction {:?}", &transaction);
				if let Some(sender) = source {
					T::Stablecoin::transfer(
						&sender,
						statement_owner,
						amount,
						ExistenceRequirement::AllowDeath,
					)?;
				} else {
					// Sender is not on-chain, therefore we simply mint to statement owner
					log::info!("[OCW] Mint to {:?} {:?}", statement_owner, amount.clone());

					// Returns negative imbalance
					let mint = T::Stablecoin::issue(amount);

					// deposit negative imbalance into the account
					T::Stablecoin::resolve_creating(statement_owner, mint);

					Self::deposit_event(Event::Minted {
						who: statement_owner.clone(),
						iban: statement_iban.clone(),
						amount,
					});
				}
			},
			TransactionType::Outgoing => {
				log::debug!("[OCW] Outgoing transaction {:?}", &transaction);

				let pallet_account = Self::account_id();
				let burn_request = reference.and_then(BurnRequests::<T>::take);

				if dest.is_some() {
					// Here we get the actual sender and receiver of the transaction
					// If user has submitted burn request, his funds are stored in the pallet's account
					// and if we detect that the reference field is populated with a burn request id,
					// we can transfer the funds from the pallet's account to the specified destination
					// account specified in the burn request.
					//
					// Otherwise, we simply transfer the funds from the statement owner to the receiver
					if let Some(request) = burn_request {
						match (
							Self::get_account_id(&request.burner),
							Self::get_account_id(&request.dest_iban),
						) {
							(Some(from), Some(to)) => {
								if from == to {
									// user is burning funds to himself, meaning he is withdrawing from ihis bank account

									// Returns negative imbalance
									let burn = T::Stablecoin::burn(amount);
									// Burn negative imbalance from the account
									if T::Stablecoin::settle(
										&pallet_account,
										burn,
										WithdrawReasons::TRANSFER,
										ExistenceRequirement::AllowDeath,
									)
									.is_ok()
									{
										Self::deposit_event(Event::Burned {
											who: pallet_account,
											iban: transaction.iban.clone(),
											amount,
										});
									}
								} else {
									// both sides are on-chain, so we can simply transfer the funds
									T::Stablecoin::transfer(
										&from,
										&to,
										amount,
										ExistenceRequirement::AllowDeath,
									)?;
								}
							},
							(Some(_account), None) => {
								// user is sending to an unknown account, so we burn the funds
								// Returns negative imbalance
								let burn = T::Stablecoin::burn(amount);
								// Burn negative imbalance from the account
								if T::Stablecoin::settle(
									&pallet_account,
									burn,
									WithdrawReasons::TRANSFER,
									ExistenceRequirement::AllowDeath,
								)
								.is_ok()
								{
									Self::deposit_event(Event::Burned {
										who: pallet_account,
										iban: transaction.iban.clone(),
										amount,
									});
								}
							},
							_ => {},
						}
					} else {
						// Receiver is on-chain, therefore we simply transfer from statement owner
						T::Stablecoin::transfer(
							statement_owner,
							&dest.unwrap(),
							amount,
							ExistenceRequirement::AllowDeath,
						)?;
					}
				} else {
					// Receiver is not on-chain, therefore we simply burn from statement owner
					log::info!("[OCW] Burn from {:?} {:?}", pallet_account, amount.clone());
					let burn_from = if burn_request.is_some() {
						pallet_account.clone()
					} else {
						statement_owner.clone()
					};

					// Returns negative imbalance
					let burn = T::Stablecoin::burn(amount);

					// Burn negative imbalance from the account
					if T::Stablecoin::settle(
						&burn_from,
						burn,
						WithdrawReasons::TRANSFER,
						ExistenceRequirement::AllowDeath,
					)
					.is_ok()
					{
						Self::deposit_event(Event::Burned {
							who: burn_from,
							iban: transaction.iban.clone(),
							amount,
						});
					}
				}
			},
			_ => {},
		}

		Ok(())
	}

	/// Process list of transactions for a given iban account
	///
	/// # Arguments
	///
	/// `iban: IbanAccount` - iban account to process transactions for
	/// `transactions: Vec<Transaction>` - list of transactions to process
	pub(crate) fn process_transactions(
		iban_account: &BankAccountOf<T>,
		#[allow(clippy::ptr_arg)] transactions: &Vec<TransactionOf<T>>,
	) -> DispatchResult {
		// Get account id of the statement owner
		let statement_owner = Self::ensure_iban_is_mapped(&iban_account.iban);

		// contains index of transaction that failed
		let mut failed_transactions: Vec<u32> = vec![];

		// Iterate over all transactions, with index

		for (i, transaction) in transactions.iter().enumerate() {
			// decode destination account id from reference
			let reference_str = core::str::from_utf8(&transaction.reference).unwrap_or("default");

			// Format of the reference is the following:
			// Purpose:AccountId; ourReference:nonce(of burn request)
			// E.g, "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:12",
			let reference_decoded: Vec<&str> = reference_str.split(';').collect();

			log::info!("[OCW] Purpose: {}", reference_decoded[0]);
			log::info!("[OCW] Reference: {}", reference_decoded[1]);

			// Source (initiator) of the transaction
			let source: Option<AccountIdOf<T>> = match transaction.tx_type {
				TransactionType::Incoming => Self::get_account_id(&transaction.iban),
				TransactionType::Outgoing => Self::get_account_id(&iban_account.iban),
				_ => None,
			};

			// Destination (recipient) of the transaction
			let dest = match AccountId32::from_ss58check(&reference_decoded[0][5..]) {
				Ok(dest) => Some(
					<T::AccountId>::decode(&mut &dest.encode()[..]).expect("Should not fail; qed"),
				),
				Err(_e) => {
					log::error!("[OCW] Failed to decode destination account from reference");
					match transaction.tx_type {
						TransactionType::Incoming => Self::get_account_id(&iban_account.iban),
						TransactionType::Outgoing => Self::get_account_id(&transaction.iban),
						_ => None,
					}
				},
			};

			// Proces transaction based on the value of reference
			// If decoding returns error, we look for the iban in the pallet storage
			let reference = reference_decoded[1][8..].parse::<u64>().ok();

			if Self::process_transaction(
				&statement_owner,
				&iban_account.iban,
				source,
				dest,
				transaction,
				reference,
			)
			.is_err()
			{
				failed_transactions.push(i as u32);
			}
		}

		Self::deposit_event(Event::StatementProcessed {
			holder: statement_owner,
			iban: iban_account.iban.clone(),
			failed_transactions,
		});

		Ok(())
	}

	/// Send unpeq request to the remote endpoint
	/// Populates the unpeg request and sends it
	///
	/// Note: This function is not called from the runtime, but from the OCW module
	///
	/// ### Arguments
	/// * `request_id` - id of the request to send
	/// * `burner` - AccountId of the burner, used to populate `purpose` field
	/// * `dest_iban` - IBAN destination
	/// * `amount` - Amount to be burned
	pub(crate) fn unpeg(
		request_id: u64,
		burner: Option<AccountIdOf<T>>,
		dest_iban: IbanOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), &'static str> {
		let remote_url = ApiUrl::<T>::get();

		let remote_url_str = core::str::from_utf8(&remote_url[..])
			.map_err(|_| "Error in converting remote_url to string")?;

		// add /unpeg to the url
		let remote_url_str = format!("{}/api-v1/unpeg", remote_url_str);

		let amount_u128 = amount.saturated_into::<u128>();

		// In the reference field, we save the request id (nonce)
		let reference = format!("{}", request_id);

		// Send request to remote endpoint
		let body = unpeg_request::<T>(
			&format!(
				"{:?}",
				AccountId32::try_from(burner.unwrap_or_else(Self::account_id).encode().as_ref())
					.expect("Should not fail")
					.to_ss58check()
			),
			amount_u128,
			&dest_iban,
			&reference,
		)
		.serialize();

		log::info!("[OCW] Sending unpeg request to {}", remote_url_str);

		let post_request = rt_offchain::http::Request::new(&remote_url_str)
			.method(rt_offchain::http::Method::Post)
			.body(vec![body])
			.add_header("Content-Type", "application/json")
			.add_header("accept", "*/*")
			.send()
			.map_err(|_| "Error in sending http POST request")?;

		log::info!("[OCW] Request sent to {}", remote_url_str);

		let response = post_request.wait().map_err(|_| "Error in waiting http response back")?;

		log::info!("[OCW] Unpeg response received {:?}", response.code);

		if response.code != 200 {
			return Err("Error in unpeg response");
		}

		Ok(())
	}

	/// Process burn requets
	///
	/// Processes registered burn requests, by sending http call to `unpeg` endpoint
	pub(crate) fn process_burn_requests() -> Result<(), &'static str> {
		for (request_id, burn_request) in <BurnRequests<T>>::iter() {
			// This is a default value, should not be processed
			if burn_request.burner == IbanOf::<T>::default() {
				return Ok(());
			}

			// Process burn requests that are either not processed yet or failed
			let dest_account = Self::get_account_id(&burn_request.dest_iban);

			// send the unpeg request
			match Self::unpeg(request_id, dest_account, burn_request.dest_iban, burn_request.amount)
			{
				Ok(_) => {
					log::info!("[OCW] Unpeq request successfull");
					BurnRequests::<T>::remove(request_id);
				},
				Err(e) => {
					log::info!("[OCW] Unpeq request failed {}", e);
					BurnRequests::<T>::remove(request_id);
				},
			};
		}

		Ok(())
	}
}
