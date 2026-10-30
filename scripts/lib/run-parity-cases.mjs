// Reference journeys: mutations are explicit so each failure has a concrete trigger.
export const linear = (workflow, nodes) => {
  workflow.nodes = nodes;
  workflow.edges = nodes.slice(1).map((node, i) => ({id:`edge-${i}`, from:nodes[i].id, to:node.id}));
  delete workflow.layout;
};
const request = (id,path="/health",extra={}) => ({id,type:"http",config:{method:"GET",path,...extra}});
const start = {id:"start",type:"start"}, end = {id:"end",type:"end"};
export function extendedCases() {
  const cases = [];
  const add=(id,file,patch,exit=0,mode="success",extra={})=>cases.push({id,file,patch,exit,mode,...extra});
  for(const [id,body,kind] of [
    ["missing-property",'{}',"extractionMissingProperty"],
    ["duplicate-key",'{"accessToken":"parity-token","accessToken":"other"}',"extractionDuplicateKey"],
    ["escaped-duplicate-key",'{"accessToken":"parity-token","access\\u0054oken":"other"}',"extractionDuplicateKey"],
    ["malformed-json",'{"accessToken":',"extractionInvalidJson"],
    ["trailing-json",'{} {}',"extractionInvalidJson"],
    ["empty-token",'{"accessToken":""}',"extractionEmptyValue"],
    ["wrong-token-type",'{"accessToken":true}',"extractionWrongType"],
    ["oversized-token",JSON.stringify({accessToken:"x".repeat(8193)}),"extractionValueLimit"],
    ["truncated-token",JSON.stringify({accessToken:"x".repeat(70000)}),"extractionResponseTruncated"],
    ["deep-json",'{"accessToken":'+ '['.repeat(65)+'true'+']'.repeat(65)+'}',"extractionJsonDepthLimit"],
    ["many-json-values",'{"accessToken":['+'0,'.repeat(50000)+'0]}',"extractionResponseTruncated"],
  ]) {
    for(const native of [false,true]) add(`${native?"native":"mapped"}-${id}`,native?"feature-showcase":"auth-login",undefined,3,"login-body",{loginBody:body,error:kind});
  }
  add("static-bearer","auth-login",d=>{
    d.identities[0].credentials={token:{source:"env",name:"STATIC_TOKEN"}};
    const w=d.workflows[0],set=w.nodes.find(n=>n.type==="setSession");
    set.config.setHeaders.authorization={source:"identity",field:"token",prefix:"Bearer "};
    linear(w,[start,set,request("check","/auth/me"),end]);
  });
  add("repeated-call-local-ids","reusable-auth",d=>{
    const w=d.workflows.find(w=>w.id==="main");linear(w,[...w.nodes.slice(0,-1),{id:"inspect-again",type:"call",config:{workflowId:"inspect-account"}},end]);
  });
  add("native-fresh-profile-credentials","feature-showcase",d=>{
    d.clientProfiles[0].headers={accept:"application/json",Authorization:"parity-stale",Cookie:"stale=parity-stale","Proxy-Authorization":"parity-stale"};
  });
  add("native-api-scoped-cookie","feature-showcase",undefined,0,"cookie-path");
  add("native-expired-cookie","feature-showcase",undefined,3,"expired-cookie",{error:"authenticationFailed"});
  add("native-rejected-credentials","feature-showcase",undefined,3,"login-rejected",{error:"authenticationFailed"});
  add("mapped-rejected-credentials","auth-login",undefined,3,"login-rejected",{error:"extractionMissingProperty"});
  add("cookie-http-error-is-not-assertion","auth-cookie",undefined,0,"login-rejected");
  add("native-auth-no-token","feature-showcase",undefined,3,"login-body",{loginBody:'{}',error:"extractionMissingProperty"});
  add("node-url-prefix-session","auth-login",d=>{d.workflows[0].nodes.find(n=>n.id==="authenticated").config.baseUrl=d.clientProfiles[0].baseUrl+"/api";});
  add("literal-template-object-is-opaque","hello-api",d=>{linear(d.workflows[0],[start,request("echo","/echo",{method:"POST",body:{kind:"jsonTemplate",fields:{value:{source:"literal",value:{source:"identity",field:"password",items:[null,1,true]}}}}}),end]);});
  add("empty-json-template","hello-api",d=>{d.workflows[0].nodes[1].config={method:"POST",path:"/echo",body:{kind:"jsonTemplate",fields:{}}};});
  add("environment-path-encoding","hello-api",d=>{d.environment={variables:{part:"a/b?c=雪",flag:true,count:42}};d.workflows[0].nodes[1].config.path="/echo/${part}?flag=${flag}&count=${count}";});
  add("node-and-profile-header-precedence","auth-login",d=>{d.clientProfiles[0].headers={"x-order":"profile"};const w=d.workflows[0];const set=w.nodes.find(n=>n.type==="setSession");set.config.setHeaders["x-order"]={source:"literal",value:"session"};w.nodes.find(n=>n.id==="authenticated").config.headers={"x-order":"node"};});
  add("remove-profile-authorization","auth-cookie",d=>{const w=d.workflows[0];d.clientProfiles[0].headers={Authorization:"parity-stale"};linear(w,[start,w.nodes[1],{id:"remove",type:"setSession",config:{removeHeaders:["authorization"]}},w.nodes[2],end]);});
  add("invalid-session-update-atomic","hello-api",d=>{d.environment={variables:{bad:"bad\r\nvalue"}};linear(d.workflows[0],[start,{id:"set",type:"setSession",config:{setHeaders:{"x-ok":{source:"literal",value:"valid"},"x-bad":{source:"environment",name:"bad"}}}},request("never"),end]);},3,"success",{error:"configuration"});
  add("rendered-body-limit","hello-api",d=>{d.environment={variables:{huge:"x".repeat(8192)}};d.workflows[0].nodes[1].config={method:"POST",path:"/echo",body:{kind:"jsonTemplate",fields:Object.fromEntries(Array.from({length:9},(_,i)=>[`v${i}`,{source:"environment",name:"huge"}]))}};},3,"success",{error:"configuration"});
  for(const [id,expression,fallback,error] of [
    ["route-no-match","response.status = 201",false,"routeNoMatch"],
    ["route-ambiguous","response.status = 200",true,"routeAmbiguous"],
    ["route-missing-body","response.body.missing = true",false,"routeSourceUnavailable"],
    ["route-invalid-type","response.body.value.status < 3",false,"routeTypeMismatch"],
  ]) add(id,"body-routing",d=>{
    const w=d.workflows[0],edges=w.edges.filter(e=>e.condition||e.default);edges[0].condition={expression};
    if(fallback){delete edges[1].default;edges[1].condition={expression};}else if(id==="route-no-match"){delete edges[1].default;edges[1].condition={expression};}
  },3,"success",{error});
  for(const phase of ["iteration","initialize"]) add(`${phase}-failure-cleanup-precedence`,"feature-showcase",d=>{
    const workflow=d.workflows.find(w=>w.id===(phase==="initialize"?"initialize":"main"));
    const node=workflow.nodes.find(n=>n.type==="http"||n.type==="authentication");node.config.path="/broken";
    d.workflows.find(w=>w.id==="cleanup").nodes.find(n=>n.type==="http").config.path="/broken-cleanup";
  },3,"broken",{error:"transport"});
  add("incomplete-response-body","hello-api",undefined,3,"incomplete",{error:"body"});
  add("http-redirect-not-followed","hello-api",undefined,0,"redirect");
  add("http-response-truncated-preview","hello-api",undefined,0,"large");
  add("http-timeout","hello-api",d=>{d.workflows[0].nodes[1].config.timeoutMs=20;},3,"wait",{error:"timeout"});
  add("literal-text-that-looks-like-json","hello-api",d=>{d.workflows[0].nodes[1].config={method:"POST",path:"/echo",body:{kind:"text",value:'{"private":"literal-text"}'}};});
  add("json-utf8-html-escaping","hello-api",d=>{d.workflows[0].nodes[1].config={method:"POST",path:"/echo",body:{kind:"json",value:{text:"<tag>&雪\u2028\u2029",literal:"\\u2028",control:"\t\n\r"}}};});
  add("mapped-null-token","auth-login",undefined,3,"login-body",{loginBody:'{"accessToken":null}',error:"extractionNullValue"});
  add("native-null-token","feature-showcase",undefined,3,"login-body",{loginBody:'{"accessToken":null}',error:"extractionWrongType"});
  add("repeated-response-headers","hello-api",undefined,0,"repeated-headers");
  add("explicit-empty-defaults","hello-api",d=>{
    d.environment={variables:{}};d.identities=[];d.assertions=[];
    d.groups[0].identityAssignment={mode:"shared"};d.groups[0].executor={kind:"once"};
    d.groups[0].lifecycle.cleanupTimeoutMs=2000;d.groups[0].lifecycle.iteration.inputs={};
    d.workflows[0].inputs={};d.workflows[0].outputs={};d.workflows[0].nodes[1].config.headers={};
  });
  for(const native of [false,true]) add(`${native?"native":"mapped"}-large-unused-array`,native?"feature-showcase":"auth-login",undefined,0,"login-body",{loginBody:JSON.stringify({accessToken:"parity-token",padding:Array(20000).fill(0)})});
  add("explicit-bearer-defaults","feature-showcase",d=>{const auth=d.workflows.find(w=>w.id==="initialize").nodes.find(n=>n.type==="authentication");auth.config.headerName="Authorization";auth.config.headerPrefix="Bearer ";});
  add("prepare-config-before-missing-secret","feature-showcase",d=>{d.clientProfiles[0].timeoutMs=300001;},2,"success",{missing:true,diagnostic:"runConfiguration"});
  add("native-custom-credential-fields","feature-showcase",d=>{
    for(const w of d.workflows)for(const n of w.nodes)if(n.type==="authentication"){n.config.usernameKey="deviceId";n.config.passwordKey="deviceSecret";}
  });
  add("native-replace-assigned-user","feature-showcase",d=>{
    const auth=d.workflows.find(w=>w.id==="initialize").nodes.find(n=>n.id==="cookie-login");auth.config.username.name="BOB_USERNAME";auth.config.password.name="BOB_PASSWORD";
  });
  add("integer-extraction-rejects-decimal-json","auth-login",d=>{const w=d.workflows[0],extract=w.nodes.find(n=>n.type==="extract");extract.config.type="integer";linear(w,[w.nodes[0],w.nodes[1],extract,end]);},3,"login-body",{loginBody:'{"accessToken":1.0}',error:"extractionWrongType"});
  add("intermediate-null-extraction","auth-login",d=>{d.workflows[0].nodes.find(n=>n.type==="extract").config.expression="response.body.parent.token";},3,"login-body",{loginBody:'{"parent":null}',error:"extractionNullValue"});
  return cases;
}
