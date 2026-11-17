pipeline {
    agent {
        docker {
            image 'maven:3'
            args '-v /tmp:/tmp'
            label 'linux'
            alwaysPull true
            reuseNode true
            registryUrl 'https://registry.example.com'
            registryCredentialsId 'creds'
            customWorkspace '/ws'
        }
    }
    stages {
        stage('A') {
            agent {
                dockerfile {
                    filename 'Dockerfile.build'
                    dir 'build'
                    additionalBuildArgs '--build-arg V=1'
                }
            }
            steps { echo 'x' }
        }
        stage('B') {
            agent {
                node {
                    label 'linux'
                    customWorkspace '/ws'
                }
            }
            steps { echo 'y' }
        }
        stage('C') {
            agent { dockerfile true }
            steps { echo 'z' }
        }
    }
}
