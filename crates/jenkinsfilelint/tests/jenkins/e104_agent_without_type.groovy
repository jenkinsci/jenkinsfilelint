pipeline {
    agent {
        // docker { image 'maven' }
    }
    stages {
        stage('Build') {
            steps {
                echo 'x'
            }
        }
    }
}
