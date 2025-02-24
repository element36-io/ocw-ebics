import React, { useState } from 'react';
import { Button, Message, Grid, Card } from 'semantic-ui-react';
import { useSubstrateState } from './substrate-lib';

export default function ProofDownloadModule() {
  const { api } = useSubstrateState();
  const [error, setError] = useState(null);

  const downloadProof = async () => {
    try {
      // Mock API call - replace with actual endpoint
      const response = await fetch('/api/get-proof');
      const proofData = await response.json();

      // Create blob and trigger download
      const blob = new Blob([JSON.stringify(proofData, null, 2)], { type: 'text/plain' });
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'zkproof.txt';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      window.URL.revokeObjectURL(url);
    } catch (err) {
      setError('Failed to download proof: ' + err.message);
    }
  };

  return (
    <Grid.Column>
      <Card fluid>
        <Card.Content>
          <Card.Header>Download ZK Proof</Card.Header>
          <Card.Description>
            Download the Zero-Knowledge proof for offline verification.
          </Card.Description>
          <Button 
            primary 
            onClick={downloadProof}
            style={{ marginTop: '1em' }}
          >
            Download Proof
          </Button>
          {error && (
            <Message negative>
              <Message.Header>Error</Message.Header>
              <p>{error}</p>
            </Message>
          )}
        </Card.Content>
      </Card>
    </Grid.Column>
  );
}
