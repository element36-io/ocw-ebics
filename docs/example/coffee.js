function doinvest() {
    var xhr = new XMLHttpRequest();
    var url = demoBaseUrl+"exchange/buy/public/for";
    console.log("calling "+url);
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/json");
    xhr.onreadystatechange = function () {
        if (xhr.readyState === 4 && xhr.status === 200) {
            var json = JSON.parse(xhr.responseText);
            showPaymentInfo(json);
        }
    };/ebics/api-v1/createOrder

    var amount=5;

    var data = JSON.stringify({
        "amount": amount,
        "symbol": "EUR36",
        "targetAddress": "0xf4560e7cb77d8b2d87f9ddce8863b3c56ffa30b8",  // https://rinkeby.aragon.org/?#/fibreetest/0xb62d5cfe67b69f9bf045a2f7ef923a384d533154/
        "targetAddressType": "CONTRACT", 
        "userIban": "CH93 0076 2011 6238 5295 7" // this is used in dev/test to simulate payments. You need to create a user, use this number when onboarding  
    });
    xhr.send(data);
}

/*


/ebics/api-v1/createOrder


{
  "amount": 1.1,
  "clearingSystemMemberId": "HYPLCH22XXX",
  "currency": "EUR",
  "msgId": "emtpy",
  "nationalPayment": true,
  "ourReference": "empty",
  "pmtInfId": "empty",
  "purpose": "0x9A0cab4250613cb8437F06ecdEc64F4644Df4D87",
  "receipientBankName": "Hypi Lenzburg AG",
  "receipientCity": "Baar",
  "receipientCountry": "CH",
  "receipientIban": "CH1230116000289537313",
  "receipientName": "element36 AG",
  "receipientStreet": "Bahnmatt",
  "receipientStreetNr": "25",
  "receipientZip": "6340",
  "sourceBic": "HYPLCH22XXX",
  "sourceIban": "CH2108307000289537320"
}

*/