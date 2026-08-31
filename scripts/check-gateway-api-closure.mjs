#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml } from "yaml";

const HTTP_METHODS = new Set(["*", "DELETE", "GET", "PATCH", "POST", "PUT"]);
const STRUCTURED_EXTENSIONS = new Set([".json", ".yaml", ".yml"]);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function readStructuredContract(filePath) {
  const content = fs.readFileSync(filePath, "utf8");
  return path.extname(filePath).toLowerCase() === ".json"
    ? JSON.parse(content)
    : parseYaml(content);
}

function routeFromOperation(operation, defaults = {}) {
  return {
    method: String(operation.method ?? "").toUpperCase(),
    path: operation.path,
    operationId: operation.operationId,
    apiAuthority:
      operation.apiAuthority ??
      operation["x-sdkwork-api-authority"] ??
      defaults.apiAuthority,
  };
}

function routesFromContract(contract) {
  if (Array.isArray(contract?.operations)) {
    return contract.operations.map((operation) =>
      routeFromOperation(operation, { apiAuthority: contract.apiAuthority }),
    );
  }
  if (Array.isArray(contract?.routes)) {
    return contract.routes.map((route) =>
      routeFromOperation(
        {
          ...route,
          method: String(route.method ?? "").toUpperCase().replace("ANY", "*"),
          apiAuthority:
            route.ownership?.apiAuthority ??
            route.apiAuthority ??
            contract.apiAuthority,
        },
        { apiAuthority: contract.apiAuthority },
      ),
    );
  }

  const routes = [];
  for (const [routePath, pathItem] of Object.entries(contract?.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      const normalizedMethod = method.toUpperCase();
      if (!HTTP_METHODS.has(normalizedMethod)) continue;
      routes.push({
        method: normalizedMethod,
        path: routePath,
        operationId: operation?.operationId,
        apiAuthority:
          operation?.["x-sdkwork-api-authority"] ??
          contract?.["x-sdkwork-api-authority"],
      });
    }
  }
  return routes;
}

function addRustStringConstants(source, namespace, stringConstants) {
  const prefix = namespace ? `${namespace}::` : "";
  const constantPattern = /(?:pub\s+)?const\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]+)"/gu;
  for (const match of source.matchAll(constantPattern)) {
    stringConstants.set(`${prefix}${match[1]}`, match[2]);
  }
}

export function routesFromRustManifest(filePath) {
  const source = fs.readFileSync(filePath, "utf8");
  const stringConstants = new Map();
  addRustStringConstants(source, null, stringConstants);

  const moduleImportPattern = /use\s+crate::([a-z_][a-z0-9_]*)\s*;/gu;
  for (const match of source.matchAll(moduleImportPattern)) {
    const moduleName = match[1];
    const candidates = [
      path.join(path.dirname(filePath), `${moduleName}.rs`),
      path.join(path.dirname(filePath), moduleName, "mod.rs"),
    ];
    const modulePath = candidates.find((candidate) => fs.existsSync(candidate));
    if (!modulePath) continue;
    addRustStringConstants(
      fs.readFileSync(modulePath, "utf8"),
      moduleName,
      stringConstants,
    );
  }

  const routes = [];
  const routePattern = /(?:HttpRoute::[a-zA-Z_][a-zA-Z0-9_]*|[a-zA-Z_][a-zA-Z0-9_]*_route)\s*\(\s*HttpMethod::(Get|Post|Put|Patch|Delete)\s*,\s*(?:"([^"]+)"|((?:[a-z_][a-z0-9_]*::)*[A-Z][A-Z0-9_]*))/gu;
  for (const match of source.matchAll(routePattern)) {
    const routePath = match[2] ?? stringConstants.get(match[3]);
    if (!routePath?.startsWith("/")) continue;
    routes.push({
      method: match[1].toUpperCase(),
      path: routePath,
      operationId: undefined,
      apiAuthority: undefined,
    });
  }
  return routes;
}

function normalizeReference(reference) {
  return String(reference ?? "").split("#", 1)[0].replaceAll("/", path.sep);
}

function resolveInside(root, reference) {
  const resolved = path.resolve(root, normalizeReference(reference));
  const relative = path.relative(root, resolved);
  if (relative.startsWith("..") || path.isAbsolute(relative)) return null;
  return resolved;
}

function configuredSurfaceName(surface) {
  const normalized = String(surface ?? "").trim().toLowerCase();
  if (["app", "backend", "open", "internal"].includes(normalized)) {
    return `${normalized}-api`;
  }
  return normalized.endsWith("-api") ? normalized : normalized;
}

function pathMatchesPrefix(routePath, prefix) {
  const normalized = prefix.replace(/\/$/u, "");
  return routePath === normalized || routePath.startsWith(`${normalized}/`);
}

function normalizedRouteShape(routePath) {
  return routePath
    .split("/")
    .map((segment) => (/^\{[^}]+\}$/u.test(segment) ? "{}" : segment))
    .join("/");
}

function embeddedDependency(dependency) {
  return (
    dependency.executableExport &&
    (dependency.runtimeModes ?? ["embedded"]).includes("embedded")
  );
}

function listSdkManifests(sdksRoot) {
  if (!fs.existsSync(sdksRoot)) return [];
  return fs
    .readdirSync(sdksRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.join(sdksRoot, entry.name, "sdk-manifest.json"))
    .filter((manifestPath) => fs.existsSync(manifestPath))
    .sort();
}

function listFiles(directory, predicate) {
  if (!fs.existsSync(directory)) return [];
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...listFiles(entryPath, predicate));
    else if (entry.isFile() && predicate(entryPath)) files.push(entryPath);
  }
  return files;
}

function resolveSdkManifest(dependencyRoot, dependency) {
  const direct = path.join(
    dependencyRoot,
    "sdks",
    dependency.sdkFamily,
    "sdk-manifest.json",
  );
  if (fs.existsSync(direct)) return direct;

  const candidates = listSdkManifests(path.join(dependencyRoot, "sdks"));
  const authorityMatch = candidates.find((candidate) => {
    const manifest = readJson(candidate);
    return manifest.apiAuthority === dependency.apiAuthority;
  });
  if (authorityMatch) return authorityMatch;

  const serviceMatch = candidates.find((candidate) => {
    const manifest = readJson(candidate);
    return manifest.apiAuthority === dependency.serviceId;
  });
  return serviceMatch ?? null;
}

function routesFromSdkManifest(dependencyRoot, manifestPath) {
  const manifest = readJson(manifestPath);
  const authorityReference = manifest.authoritySpec ?? manifest.generationInputSpec;
  if (authorityReference) {
    const authorityPath = path.resolve(path.dirname(manifestPath), authorityReference);
    const relative = path.relative(dependencyRoot, authorityPath);
    const insideDependency = !relative.startsWith("..") && !path.isAbsolute(relative);
    if (insideDependency && fs.existsSync(authorityPath)) {
      return {
        routes: routesFromContract(readStructuredContract(authorityPath)),
        sourcePath: authorityPath,
        manifest,
      };
    }
  }
  return {
    routes: routesFromContract(manifest),
    sourcePath: manifestPath,
    manifest,
  };
}

function routesFromApiContractManifest(dependencyRoot, dependency, surfaceNames) {
  const manifestPath = path.join(dependencyRoot, "apis", "manifest.json");
  if (!fs.existsSync(manifestPath)) return null;
  const manifest = readJson(manifestPath);
  const contract = (manifest.contracts ?? []).find(
    (candidate) =>
      candidate.apiAuthority === dependency.apiAuthority ||
      (candidate.apiAuthority === dependency.serviceId &&
        surfaceNames.has(candidate.surface)),
  );
  if (!contract?.path) return null;
  const contractPath = resolveInside(dependencyRoot, contract.path);
  if (!contractPath || !fs.existsSync(contractPath)) return null;
  return {
    routes: routesFromContract(readStructuredContract(contractPath)),
    sourcePath: contractPath,
  };
}

function routesFromScopedOpenApi(dependencyRoot, dependency, surfaceNames) {
  const applicationCode = dependency.workspace.replace(/^sdkwork-/u, "");
  const isCandidate = (filePath) => {
    if (!STRUCTURED_EXTENSIONS.has(path.extname(filePath).toLowerCase())) return false;
    const normalized = filePath.replaceAll(path.sep, "/").toLowerCase();
    if (!normalized.includes("openapi")) return false;
    return [...surfaceNames].some((surface) => {
      const stem = `${applicationCode}-${surface}`;
      return normalized.includes(stem) || normalized.includes(`/${surface}/`);
    });
  };
  const candidates = [];
  for (const surface of surfaceNames) {
    candidates.push(
      ...listFiles(path.join(dependencyRoot, "apis", surface), isCandidate),
    );
  }
  const cratesRoot = path.join(dependencyRoot, "crates");
  if (fs.existsSync(cratesRoot)) {
    for (const entry of fs.readdirSync(cratesRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      candidates.push(
        ...listFiles(
          path.join(cratesRoot, entry.name, "specs", "openapi"),
          isCandidate,
        ),
      );
    }
  }

  const routes = [];
  for (const candidate of candidates.sort()) {
    let contract;
    try {
      contract = readStructuredContract(candidate);
    } catch {
      continue;
    }
    for (const route of routesFromContract(contract)) {
      routes.push({ route, sourcePath: candidate });
    }
  }
  return routes;
}

function routesFromAssemblyManifest(dependencyRoot, routeCrates, errors, serviceId) {
  const routes = [];
  for (const routeCrate of routeCrates) {
    if (!routeCrate.routeManifestRef) {
      const rustCandidates = [
        path.join(dependencyRoot, routeCrate.memberDir ?? "", "src", "http_route_manifest.rs"),
        path.join(dependencyRoot, routeCrate.memberDir ?? "", "src", "lib.rs"),
      ];
      const rustPath = rustCandidates.find((candidate) => fs.existsSync(candidate));
      if (!rustPath) {
        errors.push(
          `${serviceId}: route crate ${routeCrate.packageName} has no routeManifestRef or rust route inventory`,
        );
        continue;
      }
      for (const route of routesFromRustManifest(rustPath)) {
        routes.push({ route, sourcePath: rustPath });
      }
      continue;
    }
    const manifestPath = resolveInside(dependencyRoot, routeCrate.routeManifestRef);
    if (!manifestPath) {
      errors.push(
        `${serviceId}: route manifest escapes dependency root: ${routeCrate.routeManifestRef}`,
      );
      continue;
    }
    if (!fs.existsSync(manifestPath)) {
      errors.push(
        `${serviceId}: route manifest does not exist: ${routeCrate.routeManifestRef}`,
      );
      continue;
    }
    const extension = path.extname(manifestPath).toLowerCase();
    const manifestRoutes = STRUCTURED_EXTENSIONS.has(extension)
      ? routesFromContract(readStructuredContract(manifestPath))
      : extension === ".rs"
        ? routesFromRustManifest(manifestPath)
        : [];
    for (const route of manifestRoutes) {
      routes.push({ route, sourcePath: manifestPath });
    }
  }
  return routes;
}

function discoverProviderRoutes(
  dependencyRoot,
  dependency,
  routeCrates,
  surfaceNames,
  errors,
) {
  const assemblyRoutes = routesFromAssemblyManifest(
    dependencyRoot,
    routeCrates,
    errors,
    dependency.serviceId,
  );
  if (assemblyRoutes.length > 0) return assemblyRoutes;

  const sdkManifestPath = resolveSdkManifest(dependencyRoot, dependency);
  if (sdkManifestPath) {
    const result = routesFromSdkManifest(dependencyRoot, sdkManifestPath);
    if (result.routes.length > 0) {
      return result.routes.map((route) => ({
        route,
        sourcePath: result.sourcePath,
      }));
    }
  }

  const apiContract = routesFromApiContractManifest(
    dependencyRoot,
    dependency,
    surfaceNames,
  );
  if (apiContract?.routes.length > 0) {
    return apiContract.routes.map((route) => ({
      route,
      sourcePath: apiContract.sourcePath,
    }));
  }


  const scopedOpenApiRoutes = routesFromScopedOpenApi(
    dependencyRoot,
    dependency,
    surfaceNames,
  );
  if (scopedOpenApiRoutes.length > 0) return scopedOpenApiRoutes;

  errors.push(
    `${dependency.serviceId}: selected assembly route manifests expose no exact HTTP method/path inventory`,
  );
  return [];
}

function discoverNestedAssemblyRoutes(
  workspaceRoot,
  dependencyRoot,
  dependency,
  configuredPrefixes,
  surfaceNames,
  errors,
) {
  const componentSpecPath = path.join(
    dependencyRoot,
    "crates",
    dependency.cargoDependency,
    "specs",
    "component.spec.json",
  );
  if (!fs.existsSync(componentSpecPath)) return [];

  const componentSpec = readJson(componentSpecPath);
  const requiredPorts = componentSpec.contracts?.requiredPorts ?? [];
  const nestedSurfaces = componentSpec.contracts?.dependencyApiSurfaces ?? [];
  const routes = [];

  for (const nestedSurface of nestedSurfaces) {
    const nestedPrefixes = nestedSurface.apiPrefixes ?? [];
    if (
      !nestedPrefixes.some((nestedPrefix) =>
        configuredPrefixes.some(({ prefix }) => prefix === nestedPrefix),
      )
    ) {
      continue;
    }

    const executableExport = nestedSurface.embeddedExecutableExport;
    const requiredPort = requiredPorts.find(
      (candidate) => candidate.export === executableExport,
    );
    if (!requiredPort?.provider) {
      errors.push(
        `${dependency.serviceId}: nested surface ${nestedPrefixes.join(", ")} has no matching required assembly port`,
      );
      continue;
    }

    const nestedRoot = path.join(workspaceRoot, nestedSurface.workspace ?? "");
    const assemblyManifestPath = path.join(
      nestedRoot,
      "crates",
      requiredPort.provider,
      "assembly-manifest.json",
    );
    if (!fs.existsSync(assemblyManifestPath)) {
      errors.push(
        `${dependency.serviceId}: nested assembly manifest does not exist for ${requiredPort.provider}`,
      );
      continue;
    }

    const assemblyManifest = readJson(assemblyManifestPath);
    if (assemblyManifest.packageName !== requiredPort.provider) {
      errors.push(
        `${dependency.serviceId}: nested assembly package mismatch (${assemblyManifest.packageName ?? "missing"})`,
      );
      continue;
    }
    const routeCrates = (assemblyManifest.routeCrates ?? []).filter((routeCrate) =>
      surfaceNames.has(routeCrate.surface),
    );
    if (routeCrates.length === 0) {
      errors.push(
        `${dependency.serviceId}: nested assembly ${requiredPort.provider} has no route crate for ${[...surfaceNames].sort().join(", ")}`,
      );
      continue;
    }

    const nestedDependency = {
      ...dependency,
      apiAuthority: nestedSurface.apiAuthority ?? dependency.apiAuthority,
      cargoDependency: requiredPort.provider,
    };
    routes.push(
      ...discoverProviderRoutes(
        nestedRoot,
        nestedDependency,
        routeCrates,
        surfaceNames,
        errors,
      ).map((route) => ({ ...route, dependencyRoot: nestedRoot })),
    );
  }

  return routes;
}

function validateRoute(route, dependency, sourcePath, dependencyRoot, errors) {
  const method = String(route.method ?? "").toUpperCase();
  if (!HTTP_METHODS.has(method) || typeof route.path !== "string") {
    errors.push(
      `${dependency.serviceId}: invalid route in ${path.relative(dependencyRoot, sourcePath)}`,
    );
    return null;
  }
  return { ...route, method };
}

export function validateGatewayApiClosure(root) {
  const applicationRoot = path.resolve(root);
  const workspaceRoot = path.dirname(applicationRoot);
  const component = readJson(path.join(applicationRoot, "specs/component.spec.json"));
  const dependencies = (component.dependencies ?? []).filter(embeddedDependency);
  const surfaces = component.apiSurfaces ?? [];
  const errors = [];
  const compiledRoutes = [];

  for (const dependency of dependencies) {
    const configuredPrefixes = surfaces
      .filter((surface) => surface.dependencyServiceId === dependency.serviceId)
      .map((surface) => ({ prefix: surface.prefix, surface: surface.surface }));
    if (configuredPrefixes.length === 0) {
      errors.push(`${dependency.serviceId}: no apiSurfaces entry`);
      continue;
    }

    // Runtime surface dependencies (for example the embedded IM realtime
    // plane, ADR-20260809-platform-gateway-realtime-hosting) are not API
    // assemblies: they carry no assembly-manifest.json and no route crates.
    // They still require the apiSurfaces entry above plus a cargo feature, a
    // cargo dependency, and an executable export; their runtime behavior is
    // covered by gateway integration tests.
    if (dependency.runtimeSurface === true) {
      if (
        !dependency.cargoFeature ||
        !dependency.cargoDependency ||
        !dependency.executableExport
      ) {
        errors.push(
          `${dependency.serviceId}: runtime surface requires cargoFeature, cargoDependency, and executableExport`,
        );
      }
      continue;
    }

    const dependencyRoot = path.join(workspaceRoot, dependency.workspace);
    if (!fs.existsSync(dependencyRoot)) {
      errors.push(`${dependency.serviceId}: workspace does not exist: ${dependency.workspace}`);
      continue;
    }
    const assemblyManifestPath = path.join(
      dependencyRoot,
      "crates",
      dependency.cargoDependency ?? "",
      "assembly-manifest.json",
    );
    if (!fs.existsSync(assemblyManifestPath)) {
      errors.push(
        `${dependency.serviceId}: assembly manifest does not exist for ${dependency.cargoDependency}`,
      );
      continue;
    }
    const assemblyManifest = readJson(assemblyManifestPath);
    if (assemblyManifest.packageName !== dependency.cargoDependency) {
      errors.push(
        `${dependency.serviceId}: assembly package mismatch (${assemblyManifest.packageName ?? "missing"})`,
      );
      continue;
    }

    const surfaceNames = new Set(
      configuredPrefixes.map(({ surface }) => configuredSurfaceName(surface)),
    );
    const routeCrates = (assemblyManifest.routeCrates ?? []).filter((routeCrate) =>
      surfaceNames.has(routeCrate.surface),
    );
    if (routeCrates.length === 0) {
      errors.push(
        `${dependency.serviceId}: assembly has no route crate for ${[...surfaceNames].sort().join(", ")}`,
      );
      continue;
    }

    const discoveredRoutes = [
      ...discoverProviderRoutes(
        dependencyRoot,
        dependency,
        routeCrates,
        surfaceNames,
        errors,
      ).map((route) => ({ ...route, dependencyRoot })),
      ...discoverNestedAssemblyRoutes(
        workspaceRoot,
        dependencyRoot,
        dependency,
        configuredPrefixes,
        surfaceNames,
        errors,
      ),
    ];
    const usedPrefixes = new Set();
    const dependencyRoutes = new Map();
    for (const {
      route: rawRoute,
      sourcePath,
      dependencyRoot: providerRoot,
    } of discoveredRoutes) {
      const route = validateRoute(
        rawRoute,
        dependency,
        sourcePath,
        providerRoot,
        errors,
      );
      if (!route) continue;
      const candidates = configuredPrefixes
        .filter(({ prefix }) => pathMatchesPrefix(route.path, prefix))
        .sort((left, right) => right.prefix.length - left.prefix.length);
      if (candidates.length === 0) {
        errors.push(
          `${dependency.serviceId}: ${route.method} ${route.path} is outside configured apiSurfaces`,
        );
        continue;
      }
      const selected = candidates[0];
      usedPrefixes.add(selected.prefix);
      const compiledRoute = {
        serviceId: dependency.serviceId,
        workspace: dependency.workspace,
        cargoDependency: dependency.cargoDependency,
        sdkFamily: dependency.sdkFamily,
        apiAuthority: dependency.apiAuthority,
        apiPrefix: selected.prefix,
        surface: selected.surface,
        method: route.method,
        path: route.path,
        operationId: route.operationId,
        source: path.relative(workspaceRoot, sourcePath).replaceAll(path.sep, "/"),
      };
      dependencyRoutes.set(`${route.method} ${route.path}`, compiledRoute);
    }
    compiledRoutes.push(...dependencyRoutes.values());

    for (const { prefix } of configuredPrefixes) {
      if (!usedPrefixes.has(prefix)) {
        errors.push(`${dependency.serviceId}: configured prefix ${prefix} has no provider route`);
      }
    }
  }

  const ownersByRoute = new Map();
  for (const route of compiledRoutes) {
    const key = `${route.method} ${normalizedRouteShape(route.path)}`;
    const owners = ownersByRoute.get(key) ?? new Set();
    owners.add(route.serviceId);
    ownersByRoute.set(key, owners);
  }
  const resolvedDedup = new Set();
  for (const [routeKey, owners] of ownersByRoute) {
    if (owners.size <= 1) continue;
    // Cross-module ownership resolution (API_ASSEMBLY_SPEC.md "Cross-Module
    // Composition Dedup And Collision Resolution"): the first apiSurfaces
    // declaration whose prefix matches the route path is the primary owner.
    // Non-primary duplicate contributions are skipped at runtime; they are
    // not contract violations.
    const [method, ...pathParts] = routeKey.split(" ");
    const routePath = pathParts.join(" ");
    const primary = primarySurfaceFor(surfaces, routePath);
    if (primary && owners.has(primary.dependencyServiceId)) {
      resolvedDedup.add(routeKey);
      continue;
    }
    errors.push(`${routeKey}: owned by multiple services: ${[...owners].sort().join(", ")}`);
  }

  compiledRoutes.sort(
    (left, right) =>
      left.path.localeCompare(right.path) ||
      left.method.localeCompare(right.method) ||
      left.serviceId.localeCompare(right.serviceId),
  );
  return { errors: [...new Set(errors)].sort(), routes: compiledRoutes };
}

function primarySurfaceFor(surfaces, routePath) {
  const matching = (surfaces ?? [])
    .filter((surface) => pathMatchesPrefix(routePath, surface.prefix))
    .sort((left, right) => right.prefix.length - left.prefix.length);
  return matching[0] ?? null;
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const rootIndex = process.argv.indexOf("--root");
  const root = rootIndex >= 0 ? process.argv[rootIndex + 1] : process.cwd();
  const result = validateGatewayApiClosure(root);
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`gateway API closure: ${error}`);
    process.exit(1);
  }
  console.log(
    `gateway API closure passed (${result.routes.length} method/path routes)`,
  );
}
