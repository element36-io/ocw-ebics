//! Functions related to the offchain worker communication
use crate::{types::*, *};

impl<T: Config> Pallet<T> {
	/// Fetch json from the Ebics Service API
	/// Return parsed json file
	pub(crate) fn fetch_json(endpoint: &str) -> Result<JsonValue, &'static str> {
		// fetch json value
		let remote_url = ApiUrl::<T>::get();
		let remote_url_str = core::str::from_utf8(remote_url.as_bytes_ref())
			.map_err(|_| "Error in converting remote_url to string")?;

		let final_url = format!("{}/{}", remote_url_str, endpoint);
		let pending = rt_offchain::http::Request::get(&final_url)
			.send()
			.map_err(|_| "Error in sending http GET request")?;

		let response = pending.wait().map_err(|_| "Error in waiting http response back")?;

		if response.code != 200 {
			// runtime_print!("Unexpected status code: {}", response.code);
			return Ok(JsonValue::Null);
		}

		let json_result: Vec<u8> = response.body().collect::<Vec<u8>>();

		let json_str: &str = match core::str::from_utf8(&json_result) {
			Ok(v) => v,
			Err(_e) => "Error parsing json",
		};

		log::info!("[OCW] JSON received: {}", json_str);

		let json_val = parse_json(json_str).map_err(|_| "Error in parsing json")?;

		Ok(json_val)
	}
	pub(crate) fn fetch_and_send_signed(ocw_activity: OcwActivity) -> Result<(), &'static str> {
		log::info!("[OCW] Activity: {:?}", ocw_activity);

		// get extrinsic signer
		let signer = Signer::<T, T::AuthorityId>::all_accounts();

		if !signer.can_sign() {
			return Err("No local accounts available! Please, insert your keys!");
		}

		let image_id = Risc0ImageId::<T>::get().ok_or("No image id set")?;

		// Get statements from remote endpoint
		let results = match ocw_activity {
			OcwActivity::FetchStatements => {
				let statements = Self::parse_statements()?;
				if statements.is_empty() {
					return Ok(());
				}

				let receipt_url = statements.keys().next().expect("Should not be empty");
				let statements = statements.values().cloned().collect::<Vec<(
					BankAccountOf<T>,
					BoundedVec<TransactionOf<T>, <T as Config>::MaxStatements>,
				)>>();

				Some(signer.send_signed_transaction(|_account| Call::queue_statements {
					statements:
						statements.clone().try_into().expect("convert to bounded vec, is fine"),
					receipt_url: receipt_url.clone(),
				}))
			},
			OcwActivity::VerifyAndProcessStatements => {
				let receipt_url = QueuedStatements::<T>::get()
					.ok_or("No queued statements")?
					.receipt_url
					.into_inner();

				let receipt_url = core::str::from_utf8(&receipt_url[..])
					.map_err(|_| "Error in converting receipt_url to string")?;

				let receipt = Self::fetch_json(receipt_url)?;

				if receipt.is_null() {
					return Ok(());
				}

				let (receipt, _): (Receipt, _) = serde_json_core::from_slice(&receipt.serialize())
					.map_err(|_| "Error in deserializing receipt")?;

				match receipt.verify(image_id) {
					Ok(_) => {
						Some(signer.send_signed_transaction(|_account| Call::process_statements {}))
					},
					Err(e) => {
						if matches!(receipt.inner, InnerReceipt::Fake { .. }) {
							#[cfg(any(feature = "risc0-dev-mode", test))]
							{
								Some(signer.send_signed_transaction(|_account| {
									Call::process_statements {}
								}))
							}
							#[cfg(not(any(feature = "risc0-dev-mode", test)))]
							{
								log::error!("[OCW] Receipt verification failed {:?}", e);
								None
							}
						} else {
							log::error!("[OCW] Receipt verification failed {:?}", e);
							None
						}
					},
				}
			},
			_ => None,
		};

		// Process result of the extrinsic
		if let Some(results) = results {
			for (acc, res) in &results {
				match res {
					Ok(()) => {
						log::info!("[OCW] [{:?}] Submitted tx", acc.id)
					},
					Err(e) => {
						log::error!(
							"[OCW] Failed to submit transaction from: [{:?}] reason: {:?}",
							acc.id,
							e
						);
						return Err("Failed to submit transaction");
					},
				}
			}
		}

		Ok(())
	}

	/// Parse bank statements
	///
	/// Returns:
	///
	/// - `iban_account: IbanAccount` - IBAN account that owns the statement
	/// - `incoming_txs: Vec<TransactionOf<T>>` - Incoming transactions in the statement
	/// - `outgoing_txs: Vec<TransactionOf<T>>` - Outgoing transactions in the statement
	pub(crate) fn parse_statements() -> Result<
		BTreeMap<
			StringOf<T>,
			(BankAccountOf<T>, BoundedVec<TransactionOf<T>, <T as Config>::MaxStatements>),
		>,
		&'static str,
	> {
		let json = Self::fetch_json("api-v1/bankstatements")?;

		let raw_array = json.as_array();

		if let Some(v) = raw_array {
			let mut balances = BTreeMap::new();

			for val in v.iter() {
				let as_object = val.as_object().ok_or("Error in parsing json")?;
				let receipt_url = extract_value::<Vec<u8>>("receiptUrl", as_object)
					.try_into()
					.unwrap_or_default();

				// extract iban account
				if let Ok(bank_account) = BankAccountOf::<T>::try_from(val) {
					// extract transactions
					let mut transactions =
						TransactionOf::<T>::parse_transactions(val, TransactionType::Outgoing)
							.unwrap_or_default();
					let mut incoming_transactions =
						TransactionOf::<T>::parse_transactions(val, TransactionType::Incoming)
							.unwrap_or_default();

					transactions.append(&mut incoming_transactions);

					// try pushing to balances
					balances.insert(
						receipt_url,
						(
							bank_account,
							transactions.try_into().expect("tx length should not exceed; qed"),
						),
					);
				}
			}
			Ok(balances)
		} else {
			Ok(BTreeMap::new())
		}
	}

	pub(crate) fn validate_tx_parameters() -> TransactionValidity {
		// check if we are on time
		if !Self::should_sync() {
			return InvalidTransaction::Future.into();
		}

		// check if the receipt is present

		let block_number = <frame_system::Pallet<T>>::block_number();

		ValidTransaction::with_tag_prefix("FiatRamps")
			.priority(T::UnsignedPriority::get())
			.and_provides(block_number)
			.longevity(64)
			.propagate(false)
			.build()
	}
}
