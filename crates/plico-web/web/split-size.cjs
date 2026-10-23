// File sizes are measured after wasm-bindgen, before optional wasm-opt.
const fs=require("node:fs"),path=require("node:path"),z=require("node:zlib");
const root=process.env.PLICO_SPLIT_DIR || "target/plico-web-split";
for(const variant of ["host","ui-v1","ui-v2","ui-reset"]){
 for(const file of fs.readdirSync(path.join(root,variant)).filter(f=>f.endsWith(".wasm")||f.endsWith(".js"))){
  const data=fs.readFileSync(path.join(root,variant,file));
  console.log(JSON.stringify({variant,file,raw:data.length,
   gzip:z.gzipSync(data,{level:9}).length,
   brotli:z.brotliCompressSync(data,{params:{[z.constants.BROTLI_PARAM_QUALITY]:11}}).length}));
 }
}
