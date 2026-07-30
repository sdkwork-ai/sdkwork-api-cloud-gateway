import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import path from 'node:path';

const rpcHost = '127.0.0.1';
const rpcPort = 50054;
const rpcTlsDomain = 'knowledgebase.local';
const rpcTrustDomain = 'sdkwork.internal';
const developmentSigningKey = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
const rpcPackage = 'sdkwork-intelligence-knowledgebase-rpc-bin';
const rpcBinary = 'sdkwork-intelligence-knowledgebase-rpc-bin';

const externalClientKeys = [
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CA_CERT_PATH',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_KEY_PATH',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY_FILE',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CREDENTIAL_TTL_SECONDS',
  'SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TIMEOUT_MS',
];

export function resolveEmbeddedKnowledgebaseRpcPlan({
  databaseSchema,
  databaseUrl,
  env = process.env,
  gatewayRoot,
  pgOptions,
}) {
  if (externalClientKeys.some((key) => nonEmpty(env[key]))) {
    return undefined;
  }
  if (!nonEmpty(databaseUrl)) {
    throw new Error('embedded Knowledgebase RPC requires a PostgreSQL URL');
  }
  if (!nonEmpty(databaseSchema)) {
    throw new Error('embedded Knowledgebase RPC requires a PostgreSQL schema');
  }
  if (!nonEmpty(pgOptions)) {
    throw new Error('embedded Knowledgebase RPC requires the resolved PostgreSQL search_path');
  }

  const knowledgebaseRoot = path.resolve(gatewayRoot, '..', 'sdkwork-knowledgebase');
  const runtimeRoot = path.join(gatewayRoot, '.runtime', 'codex', 'knowledgebase-rpc');
  const tlsRoot = path.join(runtimeRoot, 'tls');
  const driveStorageRoot = path.join(runtimeRoot, 'drive-objects');
  const caCertificatePath = path.join(tlsRoot, 'ca.pem');
  const caPrivateKeyPath = path.join(tlsRoot, 'ca-key.pem');
  const serverCertificatePath = path.join(tlsRoot, 'server.pem');
  const serverPrivateKeyPath = path.join(tlsRoot, 'server-key.pem');
  const clientCertificatePath = path.join(tlsRoot, 'client.pem');
  const clientPrivateKeyPath = path.join(tlsRoot, 'client-key.pem');

  return {
    clientEnv: {
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT: `https://${rpcHost}:${rpcPort}`,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CA_CERT_PATH: caCertificatePath,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH: clientCertificatePath,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_KEY_PATH: clientPrivateKeyPath,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN: rpcTlsDomain,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY:
        developmentSigningKey,
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CREDENTIAL_TTL_SECONDS: '60',
      SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TIMEOUT_MS: '5000',
    },
    hostEnv: {
      SDKWORK_KNOWLEDGEBASE_ENVIRONMENT: 'development',
      SDKWORK_KNOWLEDGEBASE_RPC_ENABLED: 'true',
      SDKWORK_KNOWLEDGEBASE_RPC_BIND_ADDR: `${rpcHost}:${rpcPort}`,
      SDKWORK_KNOWLEDGEBASE_RPC_TLS_ENABLED: 'true',
      SDKWORK_KNOWLEDGEBASE_RPC_MTLS_ENABLED: 'true',
      SDKWORK_KNOWLEDGEBASE_RPC_HEALTH_ENABLED: 'true',
      SDKWORK_KNOWLEDGEBASE_RPC_SERVER_CERT_PATH: serverCertificatePath,
      SDKWORK_KNOWLEDGEBASE_RPC_SERVER_KEY_PATH: serverPrivateKeyPath,
      SDKWORK_KNOWLEDGEBASE_RPC_CLIENT_CA_CERTIFICATE_PATH: caCertificatePath,
      SDKWORK_KNOWLEDGEBASE_RPC_SPIFFE_TRUST_DOMAIN: rpcTrustDomain,
      SDKWORK_KNOWLEDGEBASE_RPC_IM_CALLER_CONTEXT_SIGNING_KEY: developmentSigningKey,
      SDKWORK_DATABASE_URL: databaseUrl,
      SDKWORK_DATABASE_SCHEMA: databaseSchema,
      SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC: 'false',
      SDKWORK_KNOWLEDGEBASE_DRIVE_STORAGE_ROOT: driveStorageRoot,
      SDKWORK_KNOWLEDGEBASE_OPERATOR_ID: 'sdkwork-knowledgebase',
      SDKWORK_KNOWLEDGEBASE_ACTOR_ID: '1',
      PGOPTIONS: pgOptions,
    },
    knowledgebaseRoot,
    runtimeRoot,
    tls: {
      caCertificatePath,
      caPrivateKeyPath,
      clientCertificatePath,
      clientPrivateKeyPath,
      serverCertificatePath,
      serverPrivateKeyPath,
      tlsRoot,
    },
  };
}

export async function startEmbeddedKnowledgebaseRpc(plan) {
  if (!plan) {
    return undefined;
  }

  prepareDevelopmentCertificates(plan.tls);
  mkdirSync(plan.hostEnv.SDKWORK_KNOWLEDGEBASE_DRIVE_STORAGE_ROOT, { recursive: true });
  const manifestPath = path.join(plan.knowledgebaseRoot, 'Cargo.toml');
  execFileSync(
    'cargo',
    [
      'build',
      '--manifest-path',
      manifestPath,
      '-p',
      rpcPackage,
      '--bin',
      rpcBinary,
    ],
    { cwd: plan.knowledgebaseRoot, stdio: 'inherit' },
  );

  const executable = path.join(
    plan.knowledgebaseRoot,
    'target',
    'debug',
    process.platform === 'win32' ? `${rpcBinary}.exe` : rpcBinary,
  );
  if (!existsSync(executable)) {
    throw new Error(`Knowledgebase RPC executable was not built: ${executable}`);
  }

  const child = spawn(executable, [], {
    cwd: plan.knowledgebaseRoot,
    env: { ...process.env, ...plan.hostEnv },
    shell: false,
    stdio: ['ignore', 'inherit', 'inherit'],
  });
  await waitForTcpListener(child, rpcHost, rpcPort, 120_000);
  process.stdout.write(
    `[sdkwork-api-cloud-gateway] managed Knowledgebase RPC ready: ${rpcHost}:${rpcPort}\n`,
  );
  return child;
}

export async function stopEmbeddedKnowledgebaseRpc(child) {
  if (!child || child.exitCode !== null) {
    return;
  }
  child.kill();
  await Promise.race([
    new Promise((resolve) => child.once('exit', resolve)),
    new Promise((resolve) => setTimeout(resolve, 5_000)),
  ]);
}

function prepareDevelopmentCertificates(tls) {
  mkdirSync(tls.tlsRoot, { recursive: true });
  const serverRequestPath = path.join(tls.tlsRoot, 'server.csr');
  const serverExtensionsPath = path.join(tls.tlsRoot, 'server.ext');
  const clientRequestPath = path.join(tls.tlsRoot, 'client.csr');
  const clientExtensionsPath = path.join(tls.tlsRoot, 'client.ext');

  writeFileSync(
    serverExtensionsPath,
    [
      'basicConstraints=critical,CA:FALSE',
      'keyUsage=critical,digitalSignature,keyEncipherment',
      'extendedKeyUsage=serverAuth',
      `subjectAltName=DNS:${rpcTlsDomain},IP:${rpcHost}`,
      '',
    ].join('\n'),
  );
  writeFileSync(
    clientExtensionsPath,
    [
      'basicConstraints=critical,CA:FALSE',
      'keyUsage=critical,digitalSignature,keyEncipherment',
      'extendedKeyUsage=clientAuth',
      `subjectAltName=URI:spiffe://${rpcTrustDomain}/sdkwork/service/sdkwork-im`,
      '',
    ].join('\n'),
  );

  runOpenSsl([
    'req', '-x509', '-newkey', 'rsa:2048', '-sha256', '-nodes',
    '-keyout', tls.caPrivateKeyPath,
    '-out', tls.caCertificatePath,
    '-days', '30',
    '-subj', '/CN=SDKWork Local Development RPC CA',
  ]);
  runOpenSsl([
    'req', '-newkey', 'rsa:2048', '-sha256', '-nodes',
    '-keyout', tls.serverPrivateKeyPath,
    '-out', serverRequestPath,
    '-subj', `/CN=${rpcTlsDomain}`,
  ]);
  runOpenSsl([
    'x509', '-req',
    '-in', serverRequestPath,
    '-CA', tls.caCertificatePath,
    '-CAkey', tls.caPrivateKeyPath,
    '-CAcreateserial',
    '-out', tls.serverCertificatePath,
    '-days', '30',
    '-sha256',
    '-extfile', serverExtensionsPath,
  ]);
  runOpenSsl([
    'req', '-newkey', 'rsa:2048', '-sha256', '-nodes',
    '-keyout', tls.clientPrivateKeyPath,
    '-out', clientRequestPath,
    '-subj', '/CN=sdkwork-im',
  ]);
  runOpenSsl([
    'x509', '-req',
    '-in', clientRequestPath,
    '-CA', tls.caCertificatePath,
    '-CAkey', tls.caPrivateKeyPath,
    '-CAcreateserial',
    '-out', tls.clientCertificatePath,
    '-days', '30',
    '-sha256',
    '-extfile', clientExtensionsPath,
  ]);
}

function runOpenSsl(args) {
  execFileSync('openssl', args, { stdio: 'ignore' });
}

function waitForTcpListener(child, host, port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      child.off('exit', onExit);
      if (error) {
        reject(error);
      } else {
        resolve();
      }
    };
    const onExit = (code, signal) => {
      finish(
        new Error(
          `Knowledgebase RPC exited before readiness (code=${code ?? 'none'}, signal=${signal ?? 'none'})`,
        ),
      );
    };
    const probe = () => {
      const socket = createConnection({ host, port });
      socket.setTimeout(1_000);
      socket.once('connect', () => {
        socket.destroy();
        finish();
      });
      const retry = () => {
        socket.destroy();
        if (Date.now() >= deadline) {
          finish(new Error(`timed out waiting for Knowledgebase RPC at ${host}:${port}`));
          return;
        }
        setTimeout(probe, 250);
      };
      socket.once('error', retry);
      socket.once('timeout', retry);
    };
    const timer = setTimeout(
      () => finish(new Error(`timed out waiting for Knowledgebase RPC at ${host}:${port}`)),
      timeoutMs,
    );
    child.once('exit', onExit);
    probe();
  });
}

function nonEmpty(value) {
  return typeof value === 'string' && value.trim().length > 0;
}
