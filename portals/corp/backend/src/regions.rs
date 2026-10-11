//! The regions an install can be registered in, per cloud: what the register form offers, and
//! what registering checks, so a typo isn't stored as a region. The first of each list is the
//! form's default. Add a region here when a cloud opens one awzy can install in.

/// AWS's commercial regions.
const AWS: &[&str] = &[
    "us-east-2",
    "us-east-1",
    "us-west-1",
    "us-west-2",
    "ca-central-1",
    "ca-west-1",
    "mx-central-1",
    "sa-east-1",
    "eu-west-1",
    "eu-west-2",
    "eu-west-3",
    "eu-central-1",
    "eu-central-2",
    "eu-north-1",
    "eu-south-1",
    "eu-south-2",
    "il-central-1",
    "me-south-1",
    "me-central-1",
    "af-south-1",
    "ap-south-1",
    "ap-south-2",
    "ap-east-1",
    "ap-northeast-1",
    "ap-northeast-2",
    "ap-northeast-3",
    "ap-southeast-1",
    "ap-southeast-2",
    "ap-southeast-3",
    "ap-southeast-4",
    "ap-southeast-5",
    "ap-southeast-7",
];

/// Google Cloud's regions.
const GCP: &[&str] = &[
    "us-central1",
    "us-east1",
    "us-east4",
    "us-east5",
    "us-south1",
    "us-west1",
    "us-west2",
    "us-west3",
    "us-west4",
    "northamerica-northeast1",
    "northamerica-northeast2",
    "northamerica-south1",
    "southamerica-east1",
    "southamerica-west1",
    "europe-west1",
    "europe-west2",
    "europe-west3",
    "europe-west4",
    "europe-west6",
    "europe-west8",
    "europe-west9",
    "europe-west10",
    "europe-west12",
    "europe-north1",
    "europe-north2",
    "europe-central2",
    "europe-southwest1",
    "me-central1",
    "me-central2",
    "me-west1",
    "africa-south1",
    "asia-south1",
    "asia-south2",
    "asia-east1",
    "asia-east2",
    "asia-northeast1",
    "asia-northeast2",
    "asia-northeast3",
    "asia-southeast1",
    "asia-southeast2",
    "australia-southeast1",
    "australia-southeast2",
];

/// Azure's regions, by the names its CLI and Terraform use.
const AZURE: &[&str] = &[
    "eastus",
    "eastus2",
    "centralus",
    "northcentralus",
    "southcentralus",
    "westus",
    "westus2",
    "westus3",
    "canadacentral",
    "canadaeast",
    "brazilsouth",
    "northeurope",
    "westeurope",
    "uksouth",
    "ukwest",
    "francecentral",
    "germanywestcentral",
    "swedencentral",
    "switzerlandnorth",
    "norwayeast",
    "polandcentral",
    "italynorth",
    "spaincentral",
    "uaenorth",
    "qatarcentral",
    "israelcentral",
    "southafricanorth",
    "centralindia",
    "southindia",
    "eastasia",
    "southeastasia",
    "japaneast",
    "japanwest",
    "koreacentral",
    "australiaeast",
    "australiasoutheast",
];

/// The clouds an install can run in, each with its regions.
pub const CLOUDS: &[(&str, &[&str])] = &[("aws", AWS), ("gcp", GCP), ("azure", AZURE)];

/// `cloud`'s regions, or None for a cloud awzy doesn't install in.
pub fn of(cloud: &str) -> Option<&'static [&'static str]> {
    CLOUDS.iter().find(|(c, _)| *c == cloud).map(|(_, regions)| *regions)
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum RegionError {
    #[error("cloud must be aws, gcp, or azure")]
    UnknownCloud,
    #[error("{region} isn't a {cloud} region")]
    NotTheClouds { cloud: String, region: String },
}

/// Whether `region` is one of `cloud`'s, as registering an install requires.
pub fn check(cloud: &str, region: &str) -> Result<(), RegionError> {
    match of(cloud) {
        None => Err(RegionError::UnknownCloud),
        Some(regions) if !regions.contains(&region) => {
            Err(RegionError::NotTheClouds { cloud: cloud.into(), region: region.into() })
        }
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_is_registered_in_one_of_its_clouds_regions() {
        assert!(check("aws", "us-east-2").is_ok());
        assert!(check("gcp", "us-central1").is_ok());
        let wrong = check("gcp", "us-east-2").unwrap_err();
        assert_eq!(wrong.to_string(), "us-east-2 isn't a gcp region");
        assert!(check("aws", "us-esat-2").is_err(), "a typo");
        assert!(check("oracle", "us-east-2").is_err());
    }

    #[test]
    fn each_cloud_has_its_own_regions_and_no_duplicates() {
        assert_eq!(of("aws").map(|r| r[0]), Some("us-east-2"));
        assert_eq!(of("gcp").map(|r| r[0]), Some("us-central1"));
        assert!(of("aws").is_some_and(|r| !r.contains(&"us-central1")));
        assert_eq!(of("oracle"), None);
        for (cloud, regions) in CLOUDS {
            let mut sorted = regions.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), regions.len(), "{cloud} lists a region twice");
        }
    }
}
