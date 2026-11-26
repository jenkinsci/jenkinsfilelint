pipeline {
    agent {
        docker {
            image
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
