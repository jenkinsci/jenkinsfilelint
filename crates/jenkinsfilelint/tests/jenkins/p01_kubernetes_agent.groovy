pipeline {
    agent {
        kubernetes {
            yaml 'apiVersion: v1'
        }
    }
    stages {
        stage('S') { steps { echo 'x' } }
    }
}
