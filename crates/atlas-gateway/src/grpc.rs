// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! gRPC edge (PDF §5.3): a typed contract for products, served alongside REST over the same
//! `AppState`. Read RPCs plus the async volume-create path (returns a job id).

use tonic::{Request, Response, Status};

use atlas_api_types::VolumeKind;

use crate::auth::Actor;
use crate::proto::atlas_storage_server::{AtlasStorage, AtlasStorageServer};
use crate::proto::{
    Alert, Cluster, CreateVolumeReply, CreateVolumeRequest, Empty, GetJobRequest, GetVolumeRequest,
    HealthReply, HealthRequest, Job, ListAlertsReply, ListAlertsRequest, ListClustersReply,
    ListPoolsReply, ListVolumesReply, Pool, Volume,
};
use crate::state::AppState;

/// Build the tonic service for the gateway state.
pub fn service(state: AppState) -> AtlasStorageServer<GrpcService> {
    AtlasStorageServer::new(GrpcService { state })
}

pub struct GrpcService {
    state: AppState,
}

fn internal(e: impl std::fmt::Display) -> Status {
    Status::internal(e.to_string())
}

fn kind_str(k: VolumeKind) -> String {
    match k {
        VolumeKind::Block => "block",
        VolumeKind::Filesystem => "filesystem",
        VolumeKind::Object => "object",
    }
    .into()
}

fn health_str(h: atlas_api_types::Health) -> String {
    match h {
        atlas_api_types::Health::Ok => "ok",
        atlas_api_types::Health::Warn => "warn",
        atlas_api_types::Health::Critical => "critical",
        atlas_api_types::Health::Unknown => "unknown",
    }
    .into()
}

#[tonic::async_trait]
impl AtlasStorage for GrpcService {
    async fn health(&self, _req: Request<HealthRequest>) -> Result<Response<HealthReply>, Status> {
        Ok(Response::new(HealthReply {
            status: "ok".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }))
    }

    async fn list_clusters(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<ListClustersReply>, Status> {
        let clusters = atlas_inventory::list_clusters(&self.state.pool)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|c| Cluster {
                id: c.id,
                name: c.name,
                health: health_str(c.health),
                raw_capacity_bytes: c.raw_capacity_bytes.unwrap_or(0),
                used_capacity_bytes: c.used_capacity_bytes.unwrap_or(0),
                available_capacity_bytes: c.available_capacity_bytes.unwrap_or(0),
            })
            .collect();
        Ok(Response::new(ListClustersReply { clusters }))
    }

    async fn list_pools(&self, _req: Request<Empty>) -> Result<Response<ListPoolsReply>, Status> {
        let pools = atlas_inventory::list_pools(&self.state.pool)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|p| Pool {
                id: p.id,
                name: p.name,
                kind: p.kind,
                used_bytes: p.used_bytes.unwrap_or(0),
                max_bytes: p.max_bytes.unwrap_or(0),
                health: health_str(p.health),
            })
            .collect();
        Ok(Response::new(ListPoolsReply { pools }))
    }

    async fn list_volumes(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<ListVolumesReply>, Status> {
        let volumes = atlas_inventory::list_volumes(&self.state.pool)
            .await
            .map_err(internal)?
            .into_iter()
            .map(volume_to_proto)
            .collect();
        Ok(Response::new(ListVolumesReply { volumes }))
    }

    async fn get_volume(&self, req: Request<GetVolumeRequest>) -> Result<Response<Volume>, Status> {
        let id = req.into_inner().id;
        let v = atlas_inventory::get_volume(&self.state.pool, &id)
            .await
            .map_err(internal)?
            .ok_or_else(|| Status::not_found(format!("volume {id}")))?;
        Ok(Response::new(volume_to_proto(v)))
    }

    async fn create_volume(
        &self,
        req: Request<CreateVolumeRequest>,
    ) -> Result<Response<CreateVolumeReply>, Status> {
        let r = req.into_inner();
        if r.name.trim().is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        if r.size_bytes <= 0 {
            return Err(Status::invalid_argument("size_bytes must be > 0"));
        }
        let policy = (!r.policy.is_empty()).then_some(r.policy.as_str());
        let placement = atlas_policy::resolve(policy, VolumeKind::Block, None);
        let namespace = if r.namespace.is_empty() {
            "default".to_string()
        } else {
            r.namespace
        };
        let tenant_id = if r.tenant_id.is_empty() {
            "global".to_string()
        } else {
            r.tenant_id
        };
        let volume_id = atlas_common::ids::volume_id();
        let job_id = atlas_common::ids::job_id();
        let idem = atlas_common::ids::stable_id(
            "idem",
            &format!("{}|{}|{}|volume.create", tenant_id, r.name, r.size_bytes),
        );
        let spec = atlas_jobs::JobSpec::VolumeCreate {
            volume_id: volume_id.clone(),
            backend_id: CEPH_BACKEND_ID.into(),
            name: r.name,
            namespace,
            storage_class: placement.storage_class,
            access_mode: placement.access_mode,
            volume_mode: placement.volume_mode,
            size_bytes: r.size_bytes,
            kind: "block".into(),
            policy: Some(placement.intent),
            owner: None,
        };
        let job = self
            .state
            .jobs
            .enqueue(&job_id, &tenant_id, Actor::grpc_id(), spec, Some(&idem))
            .await
            .map_err(internal)?;
        Ok(Response::new(CreateVolumeReply {
            job_id: job.id,
            volume_id,
            state: job.state,
        }))
    }

    async fn get_job(&self, req: Request<GetJobRequest>) -> Result<Response<Job>, Status> {
        let id = req.into_inner().id;
        let j = atlas_inventory::jobs::get_job(&self.state.pool, &id)
            .await
            .map_err(internal)?
            .ok_or_else(|| Status::not_found(format!("job {id}")))?;
        Ok(Response::new(Job {
            id: j.id,
            job_type: j.job_type,
            state: j.state,
            progress_percent: j.progress_percent,
            error: j.error.unwrap_or_default(),
        }))
    }

    async fn list_alerts(
        &self,
        req: Request<ListAlertsRequest>,
    ) -> Result<Response<ListAlertsReply>, Status> {
        let state = req.into_inner().state;
        let filter = (!state.is_empty()).then_some(state.as_str());
        let alerts = atlas_inventory::alerts::list(&self.state.pool, filter)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|a| Alert {
                id: a.id,
                severity: a.severity,
                title: a.title,
                description: a.description,
                state: a.state,
                resource_id: a.resource_id,
            })
            .collect();
        Ok(Response::new(ListAlertsReply { alerts }))
    }
}

/// The single Ceph backend id (mirrors startup::CEPH_BACKEND_ID).
pub const CEPH_BACKEND_ID: &str = "bkd_ceph_lab";

fn volume_to_proto(v: atlas_api_types::StorageVolume) -> Volume {
    Volume {
        id: v.id,
        name: v.name,
        kind: kind_str(v.kind),
        size_bytes: v.size_bytes,
        state: v.state,
        pvc_name: v.pvc_name.unwrap_or_default(),
        storage_class: v.storage_class_name.unwrap_or_default(),
        namespace: v.kubernetes_namespace.unwrap_or_default(),
    }
}
