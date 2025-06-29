export const toast = txt => alert(txt);

export const api = (url,body,json=true)=>
  fetch(url,{
    method:"POST",
    body:JSON.stringify(body),
    headers:{ "Content-Type":"application/json" }
  }).then(r=>{
    if(!r.ok) throw r;
    return json ? r.json() : r;
  });
