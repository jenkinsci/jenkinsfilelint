// Builds, other agents and other strings are not images that are pulled.
pipeline {
    agent {
        dockerfile {
            filename 'Dockerfile.build'
        }
    }
    stages {
        stage('Build') {
            steps {
                script {
                    def app = docker.build('example/app')
                    app.push('latest')
                }
                echo 'image maven'
                sh 'docker run maven:latest mvn -v'
            }
        }
    }
}
