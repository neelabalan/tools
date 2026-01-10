use std::collections::HashMap;

trait Dedent {
    fn dedent(&self) -> String;
}

impl Dedent for str {
    fn dedent(&self) -> String {
        let min_indent = self
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        let mut result = String::new();
        for (i, line) in self.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            if !line.trim().is_empty() {
                result.push_str(&line[min_indent..]);
            }
        }
        result
    }
}

impl Dedent for String {
    fn dedent(&self) -> String {
        self.as_str().dedent()
    }
}

struct DockerFileBuilder {
    dockerfile_template_base: String,
}

struct DistroConfig {
    name: String,
    format: String,
    base_image: String,
    commands: DistroSpecificCommands,
}

struct DistroSpecificCommands {
    pkg_install: String,
    pkg_install_flags: Option<String>,
    pkg_update: String,
    docker_install: String,
    gcc_package: String,
    sysutils_packages: Vec<String>,
    mirror_setup: String,
}

impl DistroConfig {
    fn install(self, packages: Vec<String>) -> String {
        let pkgs = packages.join(" ");
        if let Some(flags) = self.commands.pkg_install_flags {
            return format!(
                "{} {} {}",
                self.commands.pkg_install, pkgs, flags
            );
        } else {
            return format!("{} {}", self.commands.pkg_install, pkgs);
        }
    }

    fn install_nosudo(self, packages: Vec<String>) -> String {
        let pkgs = packages.join(" ");
        let cmd = self.commands.pkg_install.replace("sudo ", "");
        if let Some(flags) = self.commands.pkg_install_flags {
            return format!(
                "{} {} {}",
                self.commands.pkg_install, pkgs, flags
            );
        } else {
            return format!("{} {}", cmd, pkgs);
        }
    }
}

enum Distro {
    Alma,
    AlmaMinimal,
    Debian,
    Ubuntu,
    Fedora,
}

struct DistroConfigBuilder {
    distro: Distro
}

impl DistroConfigBuilder {
    fn build(self) -> DistroConfig { 
        match self.distro {
            Distro::Debian => self.debian_distro_config(),
            Distro::Ubuntu => self.ubuntu_distro_config(),
            Distro::Alma => self.alma_distro_config(),
            Distro::AlmaMinimal => self.alma_minimal_distro_config(),
            Distro::Fedora => self.fedora_distro_config(),
        }
    }
    
    fn debian_distro_config(self) -> DistroConfig {
        DistroConfig {
            name: String::from("debian"),
            pkg_format: String::from("deb"),
            base_image: String::from("debian:bookworm"),
            commands: DistroSpecificCommands {
                pkg_install: String::from("sudo apt install -y"),
                pkg_install_flags: None,
                pkg_update: String::from("apt update && apt upgrade -y"),
                docker_install: r#"
            sudo install -m 0755 -d /etc/apt/keyrings && \\
            sudo curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc && \\
            sudo chmod a+r /etc/apt/keyrings/docker.asc && \\
            echo \\
                "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian \\
                $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \\
                sudo tee /etc/apt/sources.list.d/docker.list > /dev/null && \\
            sudo apt-get update -y && \\
            sudo apt-get install docker-ce-cli -y
            "#.dedent(),
                gcc_package: String::from("gcc"),
                sysutils_packages: vec![String::from("procps"), String::from("iproute2")],
                mirror_setup: String::from("echo \"Acquire::Retries \"3\";\" > /etc/apt/apt.conf.d/80-retries"),
            },
        }
    }
    
    fn rpm_base_commands() -> DistroSpecificCommands {
        DistroSpecificCommands {
            pkg_install: String::from("sudo dnf install -y"),
            pkg_install_flags: None,
            pkg_update: String::from("dnf update -y"),
            docker_install: r#"
            sudo dnf -y install dnf-plugins-core && \\
            sudo dnf config-manager --add-repo https://download.docker.com/linux/rhel/docker-ce.repo && \\
            sudo dnf install -y docker-ce-cli
            "#.dedent(),
            gcc_package: String::from("gcc"),
            sysutils_packages: vec![String::from("procps"), String::from("iproute")],
            mirror_setup: String::from("echo \"fastestmirror=True\" >> /etc/dnf/dnf.conf && echo \"max_parallel_downloads=10\" >> /etc/dnf/dnf.conf"),
        }
    }
    
    fn alma_distro_config(self) -> DistroConfig {
        DistroConfig {
            name: String::from("alma"),
            pkg_format: String::from("rpm"),
            base_image: String::from("almalinux:9"),
            commands: DistroSpecificCommands {
                pkg_install_flags: Some(String::from("--skip-broken")),
                ..Self::rpm_base_commands()
            },
        }
    }
    
    fn alma_minimal_distro_config(self) -> DistroConfig {
        DistroConfig {
            name: String::from("alma-minimal"),
            pkg_format: String::from("rpm"),
            base_image: String::from("almalinux:9-minimal"),
            commands: DistroSpecificCommands {
                pkg_install: String::from("sudo microdnf install -y"),
                pkg_install_flags: None,
                pkg_update: String::from("microdnf update -y"),
                docker_install: r#"
            sudo microdnf -y install dnf-plugins-core && \\
            sudo dnf config-manager --add-repo https://download.docker.com/linux/rhel/docker-ce.repo && \\
            sudo microdnf install -y docker-ce-cli
            "#.dedent(),
                mirror_setup: String::from("true"),
                ..Self::rpm_base_commands()
            },
        }
    }
    
    fn fedora_distro_config(self) -> DistroConfig {
        DistroConfig {
            name: String::from("fedora"),
            pkg_format: String::from("rpm"),
            base_image: String::from("fedora:41"),
            commands: Self::rpm_base_commands(),
        }
    }
    
    fn ubuntu_distro_config(self) -> DistroConfig {
        DistroConfig {
            name: String::from("ubuntu"),
            pkg_format: String::from("deb"),
            base_image: String::from("ubuntu:22.04"),
            commands: DistroSpecificCommands {
                pkg_install: String::from("sudo apt install -y"),
                pkg_install_flags: None,
                pkg_update: String::from("apt update && apt upgrade -y"),
                docker_install: r#"
            sudo install -m 0755 -d /etc/apt/keyrings && \\
            sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc && \\
            sudo chmod a+r /etc/apt/keyrings/docker.asc && \\
            echo \\
                "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \\
                $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \\
                sudo tee /etc/apt/sources.list.d/docker.list > /dev/null && \\
            sudo apt-get update -y && \\
            sudo apt-get install docker-ce-cli -y
            "#.dedent(),
                gcc_package: String::from("gcc"),
                sysutils_packages: vec![String::from("procps"), String::from("iproute2")],
                mirror_setup: String::from("sed -i 's|http://archive.ubuntu.com|http://mirrors.ubuntu.com|g' /etc/apt/sources.list"),
            },
        }
    }


}

// struct Dotfiles
struct Profile {
    distro: String,
    arch: String,
    user: String,
    volumes: Option<HashMap<String, String>>,
    tools: Vec<String>,
    dotfiles: Vec<String>,
}

struct Profiles {
    profiles: Vec<Profile>
}

impl DockerFileBuilder {
    fn new(mut self) -> Self {
        self.dockerfile_template_base = r#"
        # NOTE: This Dockerfile is generated. Do not edit manually.
        FROM <$>base_image
        SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
        ENV SHELL=/bin/bash

        RUN <$>mirror_configure && \
            <$>update && \
            <$>install_sudo

        ARG USERNAME=<$>username
        ARG USER_UID=1000
        ARG USER_GID=$USER_UID

        RUN groupadd --gid $USER_GID $USERNAME \
            && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
            && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
            && chmod 0440 /etc/sudoers.d/$USERNAME

        USER $USERNAME

        WORKDIR <$>workdir

        ENV HOME=<$>workdir

        <$>tool_stages

        # SecretsUsedInArgOrEnv: Do not use ARG or ENV instructions for sensitive data
        ARG PASSWORD=admin
        RUN echo "${USERNAME}:${PASSWORD}" | sudo chpasswd
    "#
        .dedent();
        self
    }
}

fn main() {
    let base_image = "ubuntu:22.04";
    let mirror_configure =
        "sed -i 's|http://archive.ubuntu.com|http://mirrors.ubuntu.com|g' /etc/apt/sources.list";
    let update = "apt-get update";
    let install_sudo = "apt-get install -y sudo";
    let workdir = "/workspace";
    let tool_stages = "RUN apt-get install -y git curl vim";

    let template = DockerFileBuilder {
        dockerfile_template_base: String::from(""),
    }
    .new()
    .dockerfile_template_base;

    let formatted = template
        .replace("<$>base_image", base_image)
        .replace("<$>mirror_configure", mirror_configure)
        .replace("<$>update", update)
        .replace("<$>install_sudo", install_sudo)
        .replace("<$>workdir", workdir)
        .replace("<$>tool_stages", tool_stages);

    println!("{}", formatted);
}
