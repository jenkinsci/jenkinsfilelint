pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                axes { axis { name 'OS'; values 'a', 'b' } }
                stages {
                    stage('T') { steps { echo 'x' } }
                    stage('T') { steps { echo 'y' } }
                }
            }
        }
    }
}
