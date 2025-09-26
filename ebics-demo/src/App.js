import React, { createRef } from 'react'
import 'semantic-ui-css/semantic.min.css'
import {
  Container,
  Dimmer,
  Grid,
  Loader,
  Message,
  Sticky,
} from 'semantic-ui-react'
import { BrowserRouter, Routes, Route } from 'react-router-dom'

import { SubstrateContextProvider, useSubstrateState } from './substrate-lib'
import { DeveloperConsole } from './substrate-lib/components'

import AccountSelector from './AccountSelector'
import FiatRampsModule from './FiatRampsModule'
import TransferTx from './TransferTx'
import ProofDownloadModule from './ProofDownload'

function Main() {
  const { apiState, apiError, keyringState } = useSubstrateState()

  const loader = text => (
    <Dimmer active>
      <Loader size="small">{text}</Loader>
    </Dimmer>
  )

  const message = errObj => (
    <Grid centered columns={2} padded>
      <Grid.Column>
        <Message
          negative
          compact
          floating
          header="Error Connecting to Substrate"
          content={`Connection to websocket '${errObj.target.url}' failed.`}
        />
      </Grid.Column>
    </Grid>
  )

  if (apiState === 'ERROR') return message(apiError)
  else if (apiState !== 'READY') return loader('Connecting to Substrate')

  if (keyringState !== 'READY') {
    return loader(
      "Loading accounts (please review any extension's authorization)"
    )
  }

  const contextRef = createRef()

  return (
    <div ref={contextRef}>
      <Sticky context={contextRef}>
        <AccountSelector />
      </Sticky>
      <Container>
        <Routes>
          {/* Main Page */}
          <Route
            path="/"
            element={
              <Grid centered stackable columns="equal">
                <Grid.Row>
                  <FiatRampsModule />
                </Grid.Row>
                <Grid.Row>
                  <TransferTx />
                </Grid.Row>
                <Grid.Row>
                  <ProofDownloadModule />
                </Grid.Row>
              </Grid>
            }
          />
          {/* Proofs-only Page */}
          <Route
            path="/proofs"
            element={
              <Grid centered stackable columns="equal">
                <Grid.Row>
                  <ProofDownloadModule />
                </Grid.Row>
              </Grid>
            }
          />
        </Routes>
      </Container>
      <DeveloperConsole />
    </div>
  )
}

export default function App() {
  return (
    <SubstrateContextProvider>
      <BrowserRouter>
        <Main />
      </BrowserRouter>
    </SubstrateContextProvider>
  )
}
