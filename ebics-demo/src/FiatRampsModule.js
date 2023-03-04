import React, { useEffect, useState } from 'react'
import { Card, Form, Grid, Input } from 'semantic-ui-react'

import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

function Main(props) {
  const { api, currentAccount } = useSubstrateState()

  // The transaction submission status
  const [status, setStatus] = useState('')

  // The currently stored value
  const [currentValue, setCurrentValue] = useState({})
  const [formIban, setFormIban] = useState('')

  console.log("CurAccount" + currentAccount?.address)

  useEffect(() => {
    let unsubscribe
    api.query.fiatRamps
      .accounts(currentAccount?.address, newValue => {
        // The storage value is an Option, so we need to check if it has a value
        if (newValue.isNone) {
          setCurrentValue({})
        } else {
          console.log("newValue: " + JSON.stringify(newValue.unwrap()["behaviour"].isKeep))
          setCurrentValue({
            iban: Buffer.from(newValue.unwrap()["iban"], "hex").toString(),
            behaviour: newValue.unwrap()["behaviour"].isKeep ? "Keep" : "Ping"
          })
        }
      })
      .then(unsub => {
        unsubscribe = unsub
      })
      .catch(console.error)

    return () => unsubscribe && unsubscribe()
  }, [api.query.fiatRamps, currentAccount?.address])

  console.log("Current Value: " + JSON.stringify(currentValue))

  return (
    <Grid.Column textAlign="center" width={16}>
      <h1>Buy me a coffee</h1>
      {!Object.keys(currentValue).length ?
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
                  inputParams: [formIban, 0x00],
                  paramFields: [true, true],
                }}
              />
            </Form.Field>
          <div style={{ overflowWrap: 'break-word' }}>{status}</div>
        </Form>
        </>
        : (
          <>
            <Card centered fluid>
              <Card.Content textAlign="center">
                <h3> Your EBICS account details</h3>
                <p>
                  <b>IBAN</b>
                </p>
                <p>{currentValue.iban}</p>
                <p>
                  <b>Account Behavior</b>
                </p>
                <p>{currentValue.behaviour}</p>
              </Card.Content>
            </Card>
            </>
        )
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
