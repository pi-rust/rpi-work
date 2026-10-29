// Resolve only this repository's development dependencies, never a neighboring checkout.
const path = require('node:path');
try {
  module.exports = require(require.resolve('playwright', { paths: [path.resolve(__dirname, '../../tools')] }));
} catch (error) {
  if (error.code !== 'MODULE_NOT_FOUND') throw error;
  throw new Error('Install browser test dependencies with: npm ci --prefix tools. Then run: npm run install-browser --prefix tools, or set CHROME_PATH.');
}
