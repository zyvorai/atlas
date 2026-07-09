// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Route, Routes } from "react-router-dom";
import { Shell } from "./shell/Shell";
import Overview from "./views/Overview";
import Volumes from "./views/Volumes";
import Rbd from "./views/Rbd";
import Snapshots from "./views/Snapshots";
import Schedules from "./views/Schedules";
import Backups from "./views/Backups";
import Buckets from "./views/Buckets";
import Alerts from "./views/Alerts";
import Activity from "./views/Activity";
import Observatory from "./views/Observatory";
import Ceph from "./views/Ceph";
import Jobs from "./views/Jobs";
import Audit from "./views/Audit";
import Tenants from "./views/Tenants";
import Access from "./views/Access";
import { Backends, Cluster, Kubernetes, Metrics, Policies } from "./views/Simple";

export default function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route path="/" element={<Overview />} />
        <Route path="/volumes" element={<Volumes />} />
        <Route path="/rbd" element={<Rbd />} />
        <Route path="/snapshots" element={<Snapshots />} />
        <Route path="/schedules" element={<Schedules />} />
        <Route path="/backups" element={<Backups />} />
        <Route path="/buckets" element={<Buckets />} />
        <Route path="/observatory" element={<Observatory />} />
        <Route path="/activity" element={<Activity />} />
        <Route path="/alerts" element={<Alerts />} />
        <Route path="/metrics" element={<Metrics />} />
        <Route path="/jobs" element={<Jobs />} />
        <Route path="/audit" element={<Audit />} />
        <Route path="/tenants" element={<Tenants />} />
        <Route path="/access" element={<Access />} />
        <Route path="/policies" element={<Policies />} />
        <Route path="/backends" element={<Backends />} />
        <Route path="/kubernetes" element={<Kubernetes />} />
        <Route path="/cluster" element={<Cluster />} />
        <Route path="/ceph" element={<Ceph />} />
        <Route path="*" element={<Overview />} />
      </Route>
    </Routes>
  );
}
