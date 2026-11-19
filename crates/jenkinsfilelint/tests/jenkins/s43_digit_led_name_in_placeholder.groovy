pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                sh "docker build --build-arg COMMIT=${COMMIT:-6ce8dcfac9a9} ."
            }
        }
    }
}
