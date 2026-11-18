pipeline {
    agent {
        docker {
            image 'a', 'b'
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
