
//var demoBaseUrl= "http://localhost:8090/cash36/" 
var demoBaseUrl= "https://demo.e36.io/api/v1/" 
var prodBaseUrl= "https://cash36.io/api/v1/" 


function simulatePayment() {
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
    var data = JSON.stringify({
        "amount": 5,
        "symbol": "EUR36",
        "targetAddress": "0xf4560e7cb77d8b2d87f9ddce8863b3c56ffa30b8",  // https://rinkeby.aragon.org/?#/fibreetest/0xb62d5cfe67b69f9bf045a2f7ef923a384d533154/
        "targetAddressType": "CONTRACT", 
        "userIban": "CH93 0076 2011 6238 5295 7" // this is used in dev/test to simulate payments. You need to create a user, use this number when onboarding  
      });
    xhr.send(data);

}
function doRealPayment() {
    var xhr = new XMLHttpRequest();
    var url = prodBaseUrl+"exchange/buy/public/for";
    console.log("calling "+url);    
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/json");
    xhr.onreadystatechange = function () {
        if (xhr.readyState === 4 && xhr.status === 200) {
            var json = JSON.parse(xhr.responseText);
            showPaymentInfo(json);
        }
    };
    var data = JSON.stringify({
        "amount": 5,
        "symbol": "EUR36", // CHF36",
        "targetAddress": "0xcde22994b8ecb3cd8b3c24426e00681ffea19f2e",  //https://mainnet.aragon.org/?#/fibreefunding/0x9477bb6b58fb3e08ad08ba30c8116ac3ac728ee9/
        "targetAddressType": "CONTRACT", 
      });
    xhr.send(data);

}

function showPaymentInfo(serverInfo) {
    console.log(JSON.stringify(serverInfo));
    document.getElementById("bankslip").innerHTML=serverInfo.qrCodeSVG;
    for(var key in serverInfo)  {
        if (document.getElementById(key)) {
            document.getElementById(key).innerHTML=serverInfo[key];
        }
    };
    document.getElementById("bankdetails").style.display="block";
    document.getElementById("explainer2").style.display="block";
}