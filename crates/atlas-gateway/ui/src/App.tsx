// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Route, Routes } from "react-router-dom";
import { Shell } from "./shell/Shell";
import Overview from "./views/Overview";
import Stub from "./views/Stub";

export default function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route path="/" element={<Overview />} />
        <Route path="/volumes" element={<Stub title="Volumes" />} />
        <Route path="/rbd" element={<Stub title="RBD Images" />} />
        <Route path="/snapshots" element={<Stub title="Snapshots" />} />
        <Route path="/schedules" element={<Stub title="Schedules" />} />
        <Route path="/backups" element={<Stub title="Backups" />} />
        <Route path="/buckets" element={<Stub title="Buckets" />} />
        <Route path="/alerts" element={<Stub title="Alerts" />} />
        <Route path="/metrics" element={<Stub title="Metrics" />} />
        <Route path="/jobs" element={<Stub title="Jobs" />} />
        <Route path="/audit" element={<Stub title="Audit" />} />
        <Route path="/tenants" element={<Stub title="Tenants" />} />
        <Route path="/access" element={<Stub title="Access" />} />
        <Route path="/policies" element={<Stub title="Policies" />} />
        <Route path="/backends" element={<Stub title="Backends" />} />
        <Route path="/kubernetes" element={<Stub title="Kubernetes" />} />
        <Route path="/cluster" element={<Stub title="Cluster" />} />
        <Route path="*" element={<Overview />} />
      </Route>
    </Routes>
  );
}
