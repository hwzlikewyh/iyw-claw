#!/usr/bin/env node
import {readFile} from "node:fs/promises"
import {pathToFileURL} from "node:url"
import path from "node:path"

const RESERVED=new Set(["HOME","USERPROFILE","APPDATA","LOCALAPPDATA","PATH","PATHEXT","SYSTEMROOT","WINDIR","COMSPEC","SHELL","NODE_OPTIONS","NODE_PATH","LD_PRELOAD","DYLD_INSERT_LIBRARIES","PYTHONPATH","PYTHONHOME","XDG_CONFIG_HOME","XDG_DATA_HOME","XDG_CACHE_HOME"])
const credential=key=>/token|secret|password|api_key|apikey|cookie|authorization/i.test(key)
const pointer=value=>typeof value==="string"&&value.startsWith("/")&&value.length<=256&&!/[\x00-\x1f]/.test(value)&&!/~(?![01])/.test(value)
function object(value,label){if(!value||typeof value!=="object"||Array.isArray(value))throw Error(`${label} must be an object`)}
function allowed(value,keys,label){object(value,label);if(Object.keys(value).some(key=>!keys.includes(key)))throw Error(`${label} contains an unsupported field`)}
function encodedBytes(value){return Buffer.byteLength(JSON.stringify(value))}
function containsCredential(value){return value&&typeof value==="object"&&Object.entries(value).some(([key,item])=>credential(key)||containsCredential(item))}
function nullDeclaration(value){if(value===null)return true;if(Array.isArray(value))return value.some(nullDeclaration);return value&&typeof value==="object"&&Object.entries(value).some(([key,item])=>key==="arguments"?(!item||Array.isArray(item)||typeof item!=="object"):nullDeclaration(item))}

export function validatePreparation(manifest){
  if(manifest.schemaVersion!==2)return
  for(const connector of manifest.components?.connectors||[]){
    if(connector.preparation===undefined)continue
    const value=connector.preparation
    if(nullDeclaration(value))throw Error("Preparation declarations cannot contain null fields")
    allowed(value,["parameters","authentication"],"preparation")
    const parameters=value.parameters??[]
    if(!Array.isArray(parameters)||parameters.length>32)throw Error("At most 32 parameters are allowed")
    const seen=new Set()
    for(const parameter of parameters)validateParameter(parameter,seen)
    if(value.authentication!==undefined)validateAuthentication(value.authentication,connector,manifest.components?.capabilities||[])
  }
}

function validateParameter(value,seen){
  allowed(value,["key","label","description","required","secret","defaultValue","placeholder"],"parameter")
  if(typeof value.key!=="string"||!/^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(value.key)||seen.has(value.key))throw Error("Invalid or duplicate parameter key")
  const upper=value.key.toUpperCase()
  if(RESERVED.has(upper)||upper.endsWith("_DATA_DIR")||/^(IYW_|CODEX_|CLAUDE_|OPENAI_|ANTHROPIC_|AWS_|AZURE_)/.test(upper))throw Error("Reserved environment variable")
  seen.add(value.key)
  if(typeof value.label!=="string"||!value.label.trim()||Buffer.byteLength(value.label)>160)throw Error("Parameter label is required (160 bytes maximum)")
  for(const [key,limit]of [["description",2000],["placeholder",512]])if(value[key]!==undefined&&(typeof value[key]!=="string"||Buffer.byteLength(value[key])>limit))throw Error(`Invalid ${key}`)
  if(["required","secret"].some(key=>value[key]!==undefined&&typeof value[key]!=="boolean"))throw Error("required and secret must be booleans")
  if(credential(value.key)&&value.secret!==true)throw Error("Credential parameters must be secret")
  if(value.defaultValue!==undefined){
    if(value.secret||typeof value.defaultValue!=="string"||Buffer.byteLength(value.defaultValue)>4096||value.defaultValue.includes("\0"))throw Error("Invalid parameter default; secrets cannot have defaults")
  }
}

function validateAuthentication(auth,connector,capabilities){
  allowed(auth,["status","authenticatedPointer","login","authorizationUrlPointer","logout"],"authentication")
  if(!pointer(auth.authenticatedPointer))throw Error("An explicit authenticatedPointer is required")
  if(auth.authorizationUrlPointer&&(!auth.login||!pointer(auth.authorizationUrlPointer)))throw Error("Invalid authorizationUrlPointer")
  if(!auth.status)throw Error("A read-only status tool is required")
  const seen=new Set()
  for(const action of [auth.status,auth.login,auth.logout].filter(value=>value!==undefined)){
    allowed(action,["toolName","arguments"],"authentication action")
    if(typeof action.toolName!=="string"||!action.toolName.trim()||Buffer.byteLength(action.toolName)>128||seen.has(action.toolName))throw Error("Invalid or repeated authentication tool")
    seen.add(action.toolName);object(action.arguments,"arguments")
    if(encodedBytes(action.arguments)>8192||JSON.stringify(action.arguments).includes("${")||containsCredential(action.arguments))throw Error("Authentication arguments cannot contain credentials or templates")
    const matches=capabilities.filter(value=>value.connectorKey===connector.key&&value.toolName===action.toolName)
    if(!matches.length||action===auth.status&&matches.some(value=>value.readOnlyHint!==true))throw Error("Authentication tools must be declared by the same connector; status must be read-only")
  }
}

async function main(){
  const root=path.resolve(process.argv[2]||".")
  const manifest=JSON.parse(await readFile(path.join(root,".iyw-plugin.json"),"utf8"))
  validatePreparation(manifest)
  process.stdout.write("Plugin preparation declarations are valid. This does not verify account login or runtime execution.\n")
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
  main().catch(error=>{process.stderr.write(`${error.message}\n`);process.exitCode=1})
}
