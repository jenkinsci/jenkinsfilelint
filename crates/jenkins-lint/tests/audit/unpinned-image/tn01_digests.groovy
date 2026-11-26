pipeline {
    agent {
        docker {
            image 'maven:3.9.9-eclipse-temurin-21@sha256:4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a394f2a9c1e8b7d6a5f4e3d2c1b'
        }
    }
    stages {
        stage('Build') {
            steps {
                script {
                    docker.image('python@sha256:0a9f8e7d6c5b4a394f2a9c1e8b7d6a5f4e3d2c1b4f2a9c1e8b7d6a5f4e3d2c1b').inside {
                        sh 'pytest'
                    }
                }
            }
        }
    }
}
