
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
    };

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