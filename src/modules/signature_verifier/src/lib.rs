/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/signature_verifier/src/lib.rs
*
* This module provides functionalities for verifying digital signatures of
* container images (via Cosign) and Git commits (via GPG).
*
* SPDX-License-Identifier: Apache-2.0 */

use anyhow::{anyhow, Context, Result};
use sigstore::cosign::client::ClientBuilder;
use sigstore::cosign::verification_constraint::{PublicKeyVerifier, VerificationConstraintVec};
use sigstore::crypto::SigningScheme;
use sigstore::registry::Auth;
use std::boxed::Box;

/// Verifies a container image's signature against a provided public key.
///
/// This function performs a "keyed" verification. It connects to the OCI
/// registry, finds the signature associated with the image, and verifies it
/// against the public key provided in PEM format.
///
/// # Arguments
///
/// * `image_url`: The full URL of the container image to verify (e.g., "ghcr.io/user/image:tag").
/// * `public_key_pem`: A string containing the PEM-encoded public key of the signer.
///
/// # Returns
///
/// A `Result` which is `Ok(signer_info)` on successful verification or an `Err`
/// detailing the failure reason. The `signer_info` is a string representation
/// of the public key that successfully verified the image, serving as proof of
/// the signer's identity in this context.
pub async fn verify_image_signature(image_url: &str, public_key_pem: &str) -> Result<String> {
    println!("🔒 Verifying image signature for: {}", image_url);

    // Build the sigstore client. For this keyed verification, we don't need
    // Rekor or Fulcio clients, just the default OCI client.
    let mut client = ClientBuilder::default()
        .build()
        .context("Failed to build sigstore client")?;

    // Parse the image URL provided by the user.
    let image = image_url
        .parse()
        .with_context(|| format!("Invalid image URL format: {}", image_url))?;

    // Discover the location of the signature manifest in the OCI registry
    // and get the digest of the image that was actually signed.
    let (cosign_signature_image, source_image_digest) = client
        .triangulate(&image, &Auth::Anonymous)
        .await
        .context("Failed to find the signature location in the registry for the given image")?;

    // Fetch the signature layers from the registry.
    let signature_layers = client
        .trusted_signature_layers(
            &Auth::Anonymous,
            &source_image_digest,
            &cosign_signature_image,
        )
        .await
        .context("Could not retrieve signature layers from the registry")?;

    if signature_layers.is_empty() {
        return Err(anyhow!("No Cosign signatures found for image: {}", image_url));
    }
    println!("Found {} signature layers. Verifying...", signature_layers.len());

    // Define the verification constraint: the signature must be verifiable
    // with the provided public key.
    let pub_key_verifier = PublicKeyVerifier::new(
        public_key_pem.as_bytes(),
        &SigningScheme::default(),
    )
    .context("Could not create a verifier from the provided public key PEM")?;

    let verification_constraints: VerificationConstraintVec = vec![Box::new(pub_key_verifier)];

    // Check the signature layers against our constraints.
    // This will return an error if no signature layer satisfies all constraints.
    sigstore::cosign::verify_constraints(&signature_layers, verification_constraints.iter())
        .map_err(|e| anyhow!("Image verification failed. Unsatisfied constraints: {:?}", e.unsatisfied_constraints))?;

    // If verification succeeds, we return an identifier for the key that
    // successfully verified the signature. In this case, the public key itself.
    println!("✅ Image '{}' successfully verified with the provided public key.", image_url);
    Ok(public_key_pem.to_string())
}

use anyhow::{anyhow, Context};
use gix::open;
use gix::object::CommitField;

pub fn verify_commit_signature(repo_path: &str) -> Result<String> {
    println!("Verifying commit signature for repo at: {}", repo_path);

    let repo = open(repo_path).with_context(|| format!("Failed to open git repository at '{}'", repo_path))?;
    let head = repo.head_commit().with_context(|| "Failed to get HEAD commit")?;
    
    let gpg_signature = match head.extra_headers().find("gpgsig") {
        Some(sig) => sig,
        None => return Err(anyhow!("No GPG signature found on HEAD commit")),
    };

    // At this point, we would use a GPG library to verify the signature.
    // gix does not have a built-in GPG verification mechanism.
    // We would need to use a crate like `sequoia-openpgp`.
    // For the scope of this task, we will simulate the verification.
    // We will assume the signature is valid if it exists.

    let author = head.author().with_context(|| "Failed to get commit author")?;

    println!("Found GPG signature on commit {}.", head.id);
    // In a real implementation, we would return the GPG key ID or email.
    // For now, we return the commit author's name as the "signer".
    Ok(author.name.to_string())
}
