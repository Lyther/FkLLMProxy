# Infrastructure Readme

## Overview

This directory contains Terraform configuration for provisioning a Google Kubernetes Engine (GKE) cluster for FkLLMProxy.

## Prerequisites

- [Terraform](https://www.terraform.io/downloads.html) >= 1.5.0
- Google Cloud SDK (`gcloud`) configured with project access

## Setup

1. **Initialize Terraform**:

   ```bash
   terraform init
   ```

2. **Configure Variables**:
   Create a `terraform.tfvars` file:

   ```hcl
   project_id  = "your-project-id"
   region      = "us-central1"
   environment = "prod"
   ```

3. **Plan**:

   ```bash
   terraform plan -out=tfplan
   ```

4. **Apply**:

   ```bash
   terraform apply tfplan
   ```

## Resources

- **GKE Cluster**: Regional cluster with auto-scaling node pool.
- **Node Pool**: `e2-medium` instances with Cloud Platform scopes (required for Vertex AI authentication via Workload Identity).

## Security Note

- **State Management**: In production, uncomment the `backend "gcs"` block in `main.tf` to use remote state.
- **Access**: Node pools have `cloud-platform` scope enabled to allow pods to authenticate with Vertex AI using Workload Identity (recommended) or Service Account impersonation.
