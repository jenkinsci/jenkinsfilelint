pipeline {
    agent any
    stages {
        stage('Test') { steps { echo 't' } }
        stage('M') {
            matrix {
                axes { axis { name 'OS'; values 'a', 'b' } }
                stages { stage('Test') { steps { echo "m ${OS}" } } }
            }
        }
    }
}
