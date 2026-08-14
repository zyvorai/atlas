// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Link, Route, Routes } from "react-router-dom";
import { Compass } from "lucide-react";
import { Shell } from "./shell/Shell";
import Overview from "./views/Overview";
import Volumes from "./views/Volumes";
import Rbd from "./views/Rbd";
import Snapshots from "./views/Snapshots";
import Schedules from "./views/Schedules";
import Backups from "./views/Backups";
import Buckets from "./views/Buckets";
import ProtectionStatus from "./views/ProtectionStatus";
import Alerts from "./views/Alerts";
import Activity from "./views/Activity";
import Observatory from "./views/Observatory";
import Ceph from "./views/Ceph";
import PoolDetail from "./views/PoolDetail";
import Jobs from "./views/Jobs";
import Audit from "./views/Audit";
import Tenants from "./views/Tenants";
import Access from "./views/Access";
import Settings from "./views/Settings";
import ApiDocs from "./views/ApiDocs";
import { Backends, Cluster, Kubernetes, Metrics, Policies } from "./views/Simple";
import Maintenance from "./views/Maintenance";
import DR from "./views/DR";
import Sources from "./views/databridge/Sources";
import Plans from "./views/databridge/Plans";
import PlanDetail from "./views/databridge/PlanDetail";
import EdgeClusters from "./views/databridge/EdgeClusters";
import Replication from "./views/databridge/Replication";
import Validation from "./views/databridge/Validation";

function NotFound() {
  return (
    <div className="grid place-items-center py-24 text-center">
      <Compass size={32} className="text-muted-foreground mb-3" />
      <div className="text-lg font-semibold mb-1">Page not found</div>
      <div className="text-sm text-muted-foreground mb-4">There's nothing at this address.</div>
      <Link to="/" className="btn btn-primary">Back to Command Deck</Link>
    </div>
  );
}

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
        <Route path="/protection" element={<ProtectionStatus />} />
        <Route path="/observatory" element={<Observatory />} />
        <Route path="/activity" element={<Activity />} />
        <Route path="/alerts" element={<Alerts />} />
        <Route path="/metrics-dashboard" element={<Metrics />} />
        <Route path="/jobs" element={<Jobs />} />
        <Route path="/audit" element={<Audit />} />
        <Route path="/tenants" element={<Tenants />} />
        <Route path="/access" element={<Access />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/api-docs" element={<ApiDocs />} />
        <Route path="/policies" element={<Policies />} />
        <Route path="/backends" element={<Backends />} />
        <Route path="/kubernetes" element={<Kubernetes />} />
        <Route path="/cluster" element={<Cluster />} />
        <Route path="/ceph" element={<Ceph />} />
        <Route path="/pools/:id" element={<PoolDetail />} />
        <Route path="/maintenance" element={<Maintenance />} />
        <Route path="/dr" element={<DR />} />
        <Route path="/databridge/sources" element={<Sources />} />
        <Route path="/databridge/plans" element={<Plans />} />
        <Route path="/databridge/plans/:id" element={<PlanDetail />} />
        <Route path="/databridge/edge-clusters" element={<EdgeClusters />} />
        <Route path="/databridge/replication" element={<Replication />} />
        <Route path="/databridge/validation" element={<Validation />} />
        <Route path="*" element={<NotFound />} />
      </Route>
    </Routes>
  );
}
