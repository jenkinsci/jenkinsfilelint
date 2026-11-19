// A tag without a digest: shown by pedantic.
pipeline {
    agent { docker { image 'maven:3.9.9-eclipse-temurin-21' } }
    stages {
        stage('Build') {
            steps {
                script {
                    docker.image('localhost:5000/tools:1.4').inside {
                        sh 'make'
                    }
                }
            }
        }
    }
}
