import { BN, formatBalance } from '@polkadot/util'
import React, { useEffect, useState } from 'react'
import { Card, Form, Grid, Input } from 'semantic-ui-react'
import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

function Main(props) {
  const { api, currentAccount, recipient } = useSubstrateState()

  // The transaction submission status
  const [status, setStatus] = useState('')
  const [totalDonations, setTotalDonations] = useState(0)

  // The currently stored value
  const [currentValue, setCurrentValue] = useState({})
  const [formIban, setFormIban] = useState('')

  useEffect(() => {
    function checkBalance() {
      let unsubscribe
      api.query.system.account(recipient.address, ({data: balance,}) => {
        // initial balance of the demo account is 1000 pEURO
        let newBalance = new BN(balance.free).sub(new BN(1000 * 10**10))
        setTotalDonations(formatBalance(
          newBalance,
          {
            decimals: 10,
            forceUnit: "pEURO",
            withUnit: "pEURO"
          }
        ))
      })
        .then(unsub => {
          unsubscribe = unsub
        }
        )
        .catch(console.error)

      return () => unsubscribe && unsubscribe()
    }

    checkBalance()
    
    let unsubscribe
    api.query.fiatRamps
      .accounts(currentAccount?.address, newValue => {
        // The storage value is an Option, so we need to check if it has a value
        if (newValue.isNone) {
          setCurrentValue({})
        } else {
          setCurrentValue({
            iban: Buffer.from(newValue.unwrap()["iban"], "hex").toString(),
          })
        }
      })
      .then(unsub => {
        unsubscribe = unsub
      })
      .catch(console.error)

    return () => unsubscribe && unsubscribe()
  }, [api.query.fiatRamps, currentAccount?.address])

  return (
    <Grid.Column textAlign="center" width={16}>
      <h1>Buy me a coffee</h1>
        <Card centered fluid>
          <Card.Content textAlign="center" style={{backgroundColor: "#ADD8E6" }}>
            <h3> My EBICS account details</h3>
            <p>
              <b>Name</b>
            </p>
            <p>{recipient.name}</p>
            <p>
              <b>AccountId</b>
            </p>
            <p>{recipient.address}</p>
            <p>
              <b>IBAN</b>
            </p>
            <p>{recipient.iban}</p>

            <p><b>Total donations</b></p>
            <p>
              {totalDonations.toString()}
            </p>
          </Card.Content>
        </Card>
      {!Object.keys(currentValue).length &&
        <>
          <Card centered fluid>
            <Card.Content textAlign="center">
              No account associated with your address:
                <br/>
                <br/>
              <b>{currentAccount?.address}</b>
            </Card.Content>
          </Card>
          <Form>
            <h3>Map your IBAN to your address</h3>
            <Form.Field>
              <Input
                label="IBAN"
                state="iban"
                type="string"
                onChange={(_, { value }) => setFormIban(value)}
              />
            </Form.Field>
            <Form.Field style={{ textAlign: 'center' }}>
              <TxButton
                label="Register Bank Account"
                type="SIGNED-TX"
                setStatus={setStatus}
                attrs={{
                  palletRpc: 'fiatRamps',
                  callable: 'createAccount',
                  inputParams: [formIban],
                  paramFields: [true],
                }}
              />
            </Form.Field>
          <div style={{ overflowWrap: 'break-word' }}>{status}</div>
        </Form>
        </>
      }
    </Grid.Column>
  )
}

export default function FiatRampsModule(props) {
  const { api } = useSubstrateState()
  return api.query.fiatRamps && api.query.fiatRamps.accounts ? (
    <Main {...props} />
  ) : null
}
