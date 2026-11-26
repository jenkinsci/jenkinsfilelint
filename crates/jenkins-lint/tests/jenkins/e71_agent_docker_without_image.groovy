pipeline {
    agent {
        docker {
            label 'linux'
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
