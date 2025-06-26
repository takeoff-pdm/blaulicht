const fs = require('fs');
const { decode } = require('@webassemblyjs/wasm-parser');

// Read the .wasm file
const buffer = fs.readFileSync('../../plugins/output.wasm');

// Parse the buffer into an AST
const ast = decode(buffer);

// Map function types
let typeEntries = [];
let funcTypes = [];
let exportMap = {};

ast.body.forEach((section) => {
  if (section.type === "TypeSection") {
    typeEntries = section.entries;
  }

  if (section.type === "FunctionSection") {
    funcTypes = section.funcTypes;
  }

  if (section.type === "ExportSection") {
    section.entries.forEach((entry) => {
      if (entry.descr.type === "Func") {
        exportMap[entry.descr.index] = entry.name;
      }
    });
  }
});

console.log("Exported Functions with Param Types:\n");

funcTypes.forEach((typeIndex, i) => {
  const funcIndex = i; // index into the function section
  const exportName = exportMap[funcIndex];
  const type = typeEntries[typeIndex];

  if (exportName) {
    const params = type.params.map(p => p.valtype).join(", ") || "none";
    const results = type.result.length > 0 ? type.result.join(", ") : "none";

    console.log(`Function "${exportName}":`);
    console.log(`  Parameters: ${params}`);
    console.log(`  Returns:    ${results}\n`);
  }
});

